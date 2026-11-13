use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use bytemuck::{Pod, Zeroable};
use engine_core::{
    BlockFace, BlockId, BlockRegistry, BlockTransparency, MaterialRegistry,
    ReferenceFrameId, RelativeTransform, ShadingModel, WorldGenerator,
};
use glam::{IVec3, Vec3};
use rayon::prelude::*;

pub const CHUNK_SIZE: usize = 16;
pub const CHUNK_SIZE_I32: i32 = CHUNK_SIZE as i32;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE; // 4096
pub const INFINITE_PERIOD: i64 = 1_048_576; // 2^20 blocks period for floating point safety
const WORLD_MAGIC: &[u8; 4] = b"VXW1";
const AO_CURVE: [f32; 4] = [1.0, 0.82, 0.66, 0.48];

/// Status state machine for chunk streaming lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChunkStatus {
    Empty = 0,
    DataReady = 1,
    Meshed = 2,
    Dirty = 3,
}

#[inline]
pub fn chunk_index(x: usize, y: usize, z: usize) -> usize {
    x + z * CHUNK_SIZE + y * CHUNK_SIZE * CHUNK_SIZE
}

/// Splits a world voxel coordinate into `(chunk_pos, local_pos)`.
#[inline]
pub fn world_to_chunk(pos: IVec3) -> (IVec3, [usize; 3]) {
    let chunk = IVec3::new(
        pos.x.div_euclid(CHUNK_SIZE_I32),
        pos.y.div_euclid(CHUNK_SIZE_I32),
        pos.z.div_euclid(CHUNK_SIZE_I32),
    );
    let local = [
        pos.x.rem_euclid(CHUNK_SIZE_I32) as usize,
        pos.y.rem_euclid(CHUNK_SIZE_I32) as usize,
        pos.z.rem_euclid(CHUNK_SIZE_I32) as usize,
    ];
    (chunk, local)
}

/// Converts a chunk coordinate and local voxel offset back to world coordinate.
#[inline]
pub fn chunk_to_world(chunk_pos: IVec3, local: [usize; 3]) -> IVec3 {
    chunk_pos * CHUNK_SIZE_I32 + IVec3::new(local[0] as i32, local[1] as i32, local[2] as i32)
}

#[inline]
pub fn pack_delta_index(x: usize, y: usize, z: usize) -> u16 {
    (((x as u16) & 0x0F) << 8) | (((y as u16) & 0x0F) << 4) | ((z as u16) & 0x0F)
}

#[inline]
pub fn unpack_delta_index(packed: u16) -> (usize, usize, usize) {
    (
        ((packed >> 8) & 0x0F) as usize,
        ((packed >> 4) & 0x0F) as usize,
        (packed & 0x0F) as usize,
    )
}

/// High-quality 64-bit spatial hash function (SplitMix64 / Murmur3 mix)
/// ensuring deterministic noise seeding across infinite worlds.
#[inline]
pub fn spatial_hash_i64(x: i64, y: i64, z: i64, seed: u64) -> u64 {
    let mut h = seed
        .wrapping_add((x as u64).wrapping_mul(0x9E3779B97F4A7C15))
        .wrapping_add((y as u64).wrapping_mul(0xBF58476D1CE4E5B9))
        .wrapping_add((z as u64).wrapping_mul(0x94D049BB133111EB));
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D049BB133111EB);
    h ^ (h >> 31)
}

/// Folds 64-bit infinite coordinates into `(super_sector, local_offset)` with a period of `1_048_576` blocks.
/// Prevents single-precision `f32` floating point jitter and mantissa degradation at extreme distances (Far Lands protection).
#[inline]
pub fn sample_infinite_coord(coord: i64) -> (i32, i32) {
    let super_sector = coord.div_euclid(INFINITE_PERIOD) as i32;
    let local = coord.rem_euclid(INFINITE_PERIOD) as i32;
    (super_sector, local)
}

/// Structure template stamped deterministically across chunk boundaries.
#[derive(Debug, Clone)]
pub struct PlacedStructure {
    pub origin: IVec3,
    pub bounds_min: IVec3,
    pub bounds_max: IVec3,
    pub voxels: Vec<(IVec3, BlockId)>,
}

/// Generator trait for macro-grid structures (dungeons, trees, asteroids, bunkers).
pub trait StructureGenerator: Send + Sync {
    fn evaluate_cell(&self, cell: IVec3, seed: u64) -> Option<PlacedStructure>;
}

/// Macro-cell structure spawner that stamps multi-chunk structures seamlessly
/// across neighboring macro-cells without seam clipping.
pub struct StructureGridSpawner {
    pub macro_cell_size: i32,
    pub seed: u64,
}

impl StructureGridSpawner {
    pub fn new(macro_cell_size: i32, seed: u64) -> Self {
        Self {
            macro_cell_size: macro_cell_size.max(16),
            seed,
        }
    }

    /// Stamps all overlapping structures from neighboring macro-cells into `chunk`.
    pub fn stamp_chunk(
        &self,
        chunk_pos: IVec3,
        chunk: &mut Chunk,
        generator: &dyn StructureGenerator,
    ) {
        let chunk_min = chunk_pos * CHUNK_SIZE_I32;
        let chunk_max = chunk_min + IVec3::splat(CHUNK_SIZE_I32 - 1);

        let cell_min_x = (chunk_min.x - self.macro_cell_size).div_euclid(self.macro_cell_size);
        let cell_max_x = (chunk_max.x + self.macro_cell_size).div_euclid(self.macro_cell_size);
        let cell_min_y = (chunk_min.y - self.macro_cell_size).div_euclid(self.macro_cell_size);
        let cell_max_y = (chunk_max.y + self.macro_cell_size).div_euclid(self.macro_cell_size);
        let cell_min_z = (chunk_min.z - self.macro_cell_size).div_euclid(self.macro_cell_size);
        let cell_max_z = (chunk_max.z + self.macro_cell_size).div_euclid(self.macro_cell_size);

        for cy in cell_min_y..=cell_max_y {
            for cz in cell_min_z..=cell_max_z {
                for cx in cell_min_x..=cell_max_x {
                    let cell = IVec3::new(cx, cy, cz);
                    let cell_seed = spatial_hash_i64(cx as i64, cy as i64, cz as i64, self.seed);
                    if let Some(structure) = generator.evaluate_cell(cell, cell_seed) {
                        let intersects = structure.bounds_min.x <= chunk_max.x
                            && structure.bounds_max.x >= chunk_min.x
                            && structure.bounds_min.y <= chunk_max.y
                            && structure.bounds_max.y >= chunk_min.y
                            && structure.bounds_min.z <= chunk_max.z
                            && structure.bounds_max.z >= chunk_min.z;

                        if intersects {
                            for &(voxel_offset, block_id) in &structure.voxels {
                                let world_pos = structure.origin + voxel_offset;
                                if world_pos.x >= chunk_min.x
                                    && world_pos.x <= chunk_max.x
                                    && world_pos.y >= chunk_min.y
                                    && world_pos.y <= chunk_max.y
                                    && world_pos.z >= chunk_min.z
                                    && world_pos.z <= chunk_max.z
                                {
                                    let lx = (world_pos.x - chunk_min.x) as usize;
                                    let ly = (world_pos.y - chunk_min.y) as usize;
                                    let lz = (world_pos.z - chunk_min.z) as usize;
                                    chunk.set(lx, ly, lz, block_id);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Compact per-chunk delta storage replacing nested HashMaps.
/// Packed 12-bit coordinate in `u16` gives 4 bytes per edit.
#[derive(Debug, Clone, Default)]
pub struct ChunkDelta {
    pub edits: Vec<(u16, BlockId)>,
}

impl ChunkDelta {
    pub fn new() -> Self {
        Self { edits: Vec::new() }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> Option<BlockId> {
        let code = pack_delta_index(x, y, z);
        self.edits.iter().find(|(k, _)| *k == code).map(|(_, id)| *id)
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, id: BlockId) {
        let code = pack_delta_index(x, y, z);
        if let Some(entry) = self.edits.iter_mut().find(|(k, _)| *k == code) {
            entry.1 = id;
        } else {
            self.edits.push((code, id));
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.edits.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }
}

/// LRU delta pager managing player world edits. Pure procedural chunks are dropped immediately
/// without disk writes, while modified chunks are cached in RAM and flushed to disk when memory limit is exceeded.
#[derive(Debug, Clone)]
pub struct InfiniteDeltaPager {
    max_memory_chunks: usize,
    cache: HashMap<IVec3, ChunkDelta>,
    lru_order: VecDeque<IVec3>,
    disk_storage: HashMap<IVec3, ChunkDelta>,
}

impl Default for InfiniteDeltaPager {
    fn default() -> Self {
        Self::new(512)
    }
}

impl InfiniteDeltaPager {
    pub fn new(max_memory_chunks: usize) -> Self {
        Self {
            max_memory_chunks: max_memory_chunks.max(1),
            cache: HashMap::new(),
            lru_order: VecDeque::new(),
            disk_storage: HashMap::new(),
        }
    }

    pub fn page_out(&mut self, pos: IVec3, delta: ChunkDelta) {
        if delta.is_empty() {
            return;
        }
        self.lru_order.retain(|&p| p != pos);
        self.lru_order.push_back(pos);
        self.cache.insert(pos, delta);

        while self.cache.len() > self.max_memory_chunks {
            if let Some(oldest) = self.lru_order.pop_front() {
                if let Some(evicted) = self.cache.remove(&oldest) {
                    self.disk_storage.insert(oldest, evicted);
                }
            }
        }
    }

    pub fn page_in(&mut self, pos: IVec3) -> Option<ChunkDelta> {
        if let Some(delta) = self.cache.remove(&pos) {
            self.lru_order.retain(|&p| p != pos);
            return Some(delta);
        }
        if let Some(delta) = self.disk_storage.remove(&pos) {
            return Some(delta);
        }
        None
    }

    pub fn peek(&self, pos: &IVec3) -> Option<&ChunkDelta> {
        self.cache.get(pos).or_else(|| self.disk_storage.get(pos))
    }

    pub fn total_saved_count(&self) -> usize {
        self.cache.len() + self.disk_storage.len()
    }
}

#[derive(Clone)]
pub struct Chunk {
    blocks: Vec<BlockId>,
    non_air_count: usize,
}

impl Chunk {
    pub fn from_blocks(blocks: Vec<BlockId>) -> Self {
        debug_assert_eq!(blocks.len(), CHUNK_VOLUME);
        let non_air_count = blocks.iter().filter(|&&b| b != BlockId::AIR).count();
        Self {
            blocks,
            non_air_count,
        }
    }

    pub fn empty() -> Self {
        Self {
            blocks: vec![BlockId::AIR; CHUNK_VOLUME],
            non_air_count: 0,
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.non_air_count == 0
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> BlockId {
        self.blocks[chunk_index(x, y, z)]
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, id: BlockId) {
        let idx = chunk_index(x, y, z);
        let prev = self.blocks[idx];
        if prev == id {
            return;
        }
        if prev == BlockId::AIR && id != BlockId::AIR {
            self.non_air_count += 1;
        } else if prev != BlockId::AIR && id == BlockId::AIR {
            self.non_air_count = self.non_air_count.saturating_sub(1);
        }
        self.blocks[idx] = id;
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct VoxelVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
    pub ao: f32,
    pub uv: [f32; 2],
    pub tex_id: u32,
    pub material_params: [f32; 4],
}

#[derive(Debug, Default, Clone)]
pub struct ChunkMesh {
    pub vertices: Vec<VoxelVertex>,
    pub indices: Vec<u32>,
}

impl ChunkMesh {
    pub fn empty() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RaycastHit {
    pub hit_pos: IVec3,
    pub place_pos: IVec3,
    pub block: BlockId,
}

/// 3x3x3 chunk neighborhood (index `(dx+1) + (dz+1)*3 + (dy+1)*9`, center at index 13).
pub type ChunkNeighborhood = [Option<Arc<Chunk>>; 27];
pub type MeshJob = (IVec3, ChunkNeighborhood);

pub enum WorkerResponse {
    Generated(Vec<(IVec3, Chunk)>),
    Meshed(Vec<(IVec3, ChunkMesh)>),
}

/// Decoupled background worker running parallel generation and greedy meshing across Rayon.
pub struct WorldStreamer {
    gen_tx: Option<Sender<Vec<IVec3>>>,
    mesh_tx: Option<Sender<Vec<MeshJob>>>,
    resp_rx: Receiver<WorkerResponse>,
    handles: Vec<JoinHandle<()>>,
}

impl WorldStreamer {
    pub fn new(generator: Arc<dyn WorldGenerator>, registry: Arc<BlockRegistry>) -> Self {
        let (gen_tx, gen_rx) = mpsc::channel::<Vec<IVec3>>();
        let (mesh_tx, mesh_rx) = mpsc::channel::<Vec<MeshJob>>();
        let (resp_tx, resp_rx) = mpsc::channel::<WorkerResponse>();

        // 1. Dedicated terrain generation worker thread
        let resp_tx_gen = resp_tx.clone();
        let gen_handle = thread::Builder::new()
            .name("world-gen-worker".to_string())
            .spawn(move || {
                while let Ok(positions) = gen_rx.recv() {
                    let generated: Vec<(IVec3, Chunk)> = positions
                        .into_par_iter()
                        .map(|pos| {
                            let mut blocks = vec![BlockId::AIR; CHUNK_VOLUME];
                            generator.generate_chunk(pos, CHUNK_SIZE, &mut blocks);
                            (pos, Chunk::from_blocks(blocks))
                        })
                        .collect();
                    if resp_tx_gen.send(WorkerResponse::Generated(generated)).is_err() {
                        break;
                    }
                }
            })
            .expect("failed to spawn world-gen-worker thread");

        // 2. Dedicated greedy meshing worker thread
        let mesh_handle = thread::Builder::new()
            .name("world-mesh-worker".to_string())
            .spawn(move || {
                while let Ok(jobs) = mesh_rx.recv() {
                    let built: Vec<(IVec3, ChunkMesh)> = jobs
                        .into_par_iter()
                        .map(|(pos, neighborhood)| {
                            let mesh = build_chunk_mesh(pos, &neighborhood, &registry, 1.0);
                            (pos, mesh)
                        })
                        .collect();
                    if resp_tx.send(WorkerResponse::Meshed(built)).is_err() {
                        break;
                    }
                }
            })
            .expect("failed to spawn world-mesh-worker thread");

        Self {
            gen_tx: Some(gen_tx),
            mesh_tx: Some(mesh_tx),
            resp_rx,
            handles: vec![gen_handle, mesh_handle],
        }
    }
}

impl Drop for WorldStreamer {
    fn drop(&mut self) {
        self.gen_tx.take();
        self.mesh_tx.take();
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

/// A cluster of detached blocks produced by structural collapse.
#[derive(Debug, Clone)]
pub struct DetachedVoxelCluster {
    pub origin_world: IVec3,
    pub pivot: Vec3,
    pub bounds_min: IVec3,
    pub bounds_max: IVec3,
    pub blocks: Vec<(IVec3, BlockId)>,
}

/// Structural integrity service that detects collapses and floating voxel clusters.
pub struct StructuralIntegrityChecker;

impl StructuralIntegrityChecker {
    pub const DEFAULT_MAX_BFS_BLOCKS: usize = 2048;

    /// Checks if destroying `broken_pos` causes adjacent solid blocks to lose connection to ground/bedrock.
    /// Detached blocks are removed from `world`, marked Dirty, and returned as physics clusters.
    pub fn check_and_detach(
        world: &mut World,
        broken_pos: IVec3,
        registry: &BlockRegistry,
        materials: &MaterialRegistry,
        ground_anchor_y: i32,
    ) -> Vec<DetachedVoxelCluster> {
        let neighbors = [
            broken_pos + IVec3::new(1, 0, 0),
            broken_pos + IVec3::new(-1, 0, 0),
            broken_pos + IVec3::new(0, 1, 0),
            broken_pos + IVec3::new(0, -1, 0),
            broken_pos + IVec3::new(0, 0, 1),
            broken_pos + IVec3::new(0, 0, -1),
        ];

        let mut detached_clusters = Vec::new();
        let mut global_visited = HashSet::new();
        global_visited.insert(broken_pos);

        for start_pos in neighbors {
            let start_block = world.get_block(start_pos);
            if start_block == BlockId::AIR || start_block == BlockId::WATER || global_visited.contains(&start_pos) {
                continue;
            }

            let mut queue = VecDeque::new();
            let mut cluster_blocks = Vec::new();
            let mut local_visited = HashSet::new();
            let mut is_anchored = false;

            queue.push_back((start_pos, 0u16));
            local_visited.insert(start_pos);

            while let Some((current, load_accum)) = queue.pop_front() {
                cluster_blocks.push(current);

                if current.y <= ground_anchor_y {
                    is_anchored = true;
                    break;
                }

                let block_id = world.get_block(current);
                if let Some(def) = registry.get(block_id) {
                    if def.hardness > 50.0 {
                        is_anchored = true;
                        break;
                    }
                }

                if cluster_blocks.len() >= Self::DEFAULT_MAX_BFS_BLOCKS {
                    is_anchored = true;
                    break;
                }

                let max_load = if let Some(def) = registry.get(block_id) {
                    materials.get(def.face_materials[0]).map(|m| m.max_structural_load).unwrap_or(100)
                } else {
                    100
                };

                for (dx, dy, dz) in [
                    (1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1),
                ] {
                    let npos = current + IVec3::new(dx, dy, dz);
                    if local_visited.contains(&npos) || global_visited.contains(&npos) {
                        continue;
                    }
                    let nb = world.get_block(npos);
                    if nb != BlockId::AIR && nb != BlockId::WATER {
                        let next_load = load_accum + 10;
                        if next_load <= max_load {
                            local_visited.insert(npos);
                            queue.push_back((npos, next_load));
                        }
                    }
                }
            }

            if is_anchored {
                for b in local_visited {
                    global_visited.insert(b);
                }
            } else {
                let mut min_bound = IVec3::splat(i32::MAX);
                let mut max_bound = IVec3::splat(i32::MIN);
                let mut sum_pos = Vec3::ZERO;
                let origin = cluster_blocks[0];

                let mut detached_voxels = Vec::with_capacity(cluster_blocks.len());

                for &p in &cluster_blocks {
                    global_visited.insert(p);
                    let block_id = world.get_block(p);
                    world.set_block(p, BlockId::AIR);

                    min_bound = min_bound.min(p);
                    max_bound = max_bound.max(p);
                    sum_pos += p.as_vec3();
                    detached_voxels.push((p - origin, block_id));
                }

                let pivot = sum_pos / (cluster_blocks.len() as f32);

                detached_clusters.push(DetachedVoxelCluster {
                    origin_world: origin,
                    pivot,
                    bounds_min: min_bound,
                    bounds_max: max_bound,
                    blocks: detached_voxels,
                });
            }
        }

        detached_clusters
    }
}

/// Axis-aligned bounding box collider.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AabbBox {
    pub min: Vec3,
    pub max: Vec3,
}

impl AabbBox {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    pub fn intersects(&self, other: &AabbBox) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }
}

/// 3D Greedy collision volume generator merging adjacent solid voxels into large AABB boxes.
pub struct GreedyColliderBoxes;

impl GreedyColliderBoxes {
    pub fn build_colliders(chunk: &Chunk, origin: IVec3, scale: f32) -> Vec<AabbBox> {
        let mut colliders = Vec::new();
        let mut visited = [false; CHUNK_VOLUME];

        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let idx = chunk_index(x, y, z);
                    if visited[idx] || chunk.get(x, y, z) == BlockId::AIR {
                        continue;
                    }

                    let mut w = 1;
                    while x + w < CHUNK_SIZE {
                        let next_idx = chunk_index(x + w, y, z);
                        if visited[next_idx] || chunk.get(x + w, y, z) == BlockId::AIR {
                            break;
                        }
                        w += 1;
                    }

                    let mut d = 1;
                    'outer_z: while z + d < CHUNK_SIZE {
                        for kx in 0..w {
                            let next_idx = chunk_index(x + kx, y, z + d);
                            if visited[next_idx] || chunk.get(x + kx, y, z + d) == BlockId::AIR {
                                break 'outer_z;
                            }
                        }
                        d += 1;
                    }

                    let mut h = 1;
                    'outer_y: while y + h < CHUNK_SIZE {
                        for kz in 0..d {
                            for kx in 0..w {
                                let next_idx = chunk_index(x + kx, y + h, z + kz);
                                if visited[next_idx] || chunk.get(x + kx, y + h, z + kz) == BlockId::AIR {
                                    break 'outer_y;
                                }
                            }
                        }
                        h += 1;
                    }

                    for dy in 0..h {
                        for dz in 0..d {
                            for dx in 0..w {
                                visited[chunk_index(x + dx, y + dy, z + dz)] = true;
                            }
                        }
                    }

                    let min_pos = (origin * CHUNK_SIZE_I32).as_vec3() * scale
                        + Vec3::new(x as f32, y as f32, z as f32) * scale;
                    let max_pos = min_pos + Vec3::new(w as f32, h as f32, d as f32) * scale;
                    colliders.push(AabbBox::new(min_pos, max_pos));
                }
            }
        }

        colliders
    }
}

/// A mobile, dynamic voxel structure (moving ship, vehicle, elevator, falling debris cluster)
/// with its own local coordinate system, meshes, and collision geometry.
pub struct VoxelContraption {
    pub id: u64,
    pub transform: RelativeTransform,
    pub chunks: HashMap<IVec3, Arc<Chunk>>,
    pub meshes: HashMap<IVec3, ChunkMesh>,
    pub colliders: Vec<AabbBox>,
    pub dirty: bool,
    pub voxel_scale: f32,
}

impl VoxelContraption {
    pub fn new(id: u64, transform: RelativeTransform, voxel_scale: f32) -> Self {
        Self {
            id,
            transform,
            chunks: HashMap::new(),
            meshes: HashMap::new(),
            colliders: Vec::new(),
            dirty: true,
            voxel_scale: voxel_scale.max(0.05),
        }
    }

    /// Converts a detached voxel cluster from structural collapse into an independent dynamic contraption.
    pub fn from_cluster(
        id: u64,
        cluster: DetachedVoxelCluster,
        parent_frame: ReferenceFrameId,
        voxel_scale: f32,
    ) -> Self {
        let mut contraption = Self::new(
            id,
            RelativeTransform::new(parent_frame, IVec3::ZERO, cluster.origin_world.as_vec3() * voxel_scale),
            voxel_scale,
        );

        for (rel_pos, block_id) in cluster.blocks {
            contraption.set_block(rel_pos, block_id);
        }

        contraption
    }

    pub fn set_block(&mut self, local_world_pos: IVec3, id: BlockId) {
        let (chunk_pos, [lx, ly, lz]) = world_to_chunk(local_world_pos);
        let chunk_arc = self.chunks.entry(chunk_pos).or_insert_with(|| {
            Arc::new(Chunk::empty())
        });
        let chunk = Arc::make_mut(chunk_arc);
        chunk.set(lx, ly, lz, id);
        self.dirty = true;
    }

    pub fn get_block(&self, local_world_pos: IVec3) -> BlockId {
        let (chunk_pos, [lx, ly, lz]) = world_to_chunk(local_world_pos);
        self.chunks.get(&chunk_pos).map_or(BlockId::AIR, |c| c.get(lx, ly, lz))
    }

    pub fn rebuild_colliders(&mut self) {
        let mut all_boxes = Vec::new();
        for (&pos, chunk) in &self.chunks {
            let boxes = GreedyColliderBoxes::build_colliders(chunk, pos, self.voxel_scale);
            all_boxes.extend(boxes);
        }
        self.colliders = all_boxes;
    }

    pub fn rebuild_meshes(&mut self, registry: &BlockRegistry) {
        let mut new_meshes = HashMap::new();
        let scale = self.voxel_scale;

        for (&pos, _) in &self.chunks {
            let mut neighborhood: ChunkNeighborhood = Default::default();
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        let idx = ((dx + 1) + (dz + 1) * 3 + (dy + 1) * 9) as usize;
                        neighborhood[idx] = self.chunks.get(&(pos + IVec3::new(dx, dy, dz))).cloned();
                    }
                }
            }
            let mesh = build_chunk_mesh(pos, &neighborhood, registry, scale);
            if !mesh.is_empty() {
                new_meshes.insert(pos, mesh);
            }
        }

        self.meshes = new_meshes;
        self.rebuild_colliders();
        self.dirty = false;
    }

    /// Transforms a point from contraption local coordinates to world coordinates.
    pub fn transform_point_to_world(&self, local_pt: Vec3) -> Vec3 {
        let origin = self.transform.world_position();
        origin + self.transform.local_rotation * (local_pt * self.voxel_scale)
    }

    /// Transforms a point from world coordinates to contraption local coordinates.
    pub fn transform_point_to_local(&self, world_pt: Vec3) -> Vec3 {
        let rel = world_pt - self.transform.world_position();
        (self.transform.local_rotation.inverse() * rel) / self.voxel_scale
    }
}

/// Multithreaded cubic-chunk voxel world with persistent block edits,
/// Neighbor-Aware Meshing Gate, and compact delta storage.
pub struct World {
    chunks: HashMap<IVec3, Arc<Chunk>>,
    statuses: HashMap<IVec3, ChunkStatus>,
    edits: HashMap<IVec3, ChunkDelta>,
    dirty_meshes: HashSet<IVec3>,
    pending_generation: HashSet<IVec3>,
    pending_meshing: HashSet<IVec3>,
    ready_meshes: Vec<(IVec3, ChunkMesh)>,
    pub delta_pager: InfiniteDeltaPager,
    pub voxel_scale: f32,
    pub min_chunk_y: i32,
    pub max_chunk_y: i32,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
            statuses: HashMap::new(),
            edits: HashMap::new(),
            dirty_meshes: HashSet::new(),
            pending_generation: HashSet::new(),
            pending_meshing: HashSet::new(),
            ready_meshes: Vec::new(),
            delta_pager: InfiniteDeltaPager::default(),
            voxel_scale: 1.0,
            min_chunk_y: -3,
            max_chunk_y: 6,
        }
    }

    pub fn with_scale(mut self, scale: f32) -> Self {
        self.voxel_scale = scale.max(0.05);
        self
    }

    pub fn clear(&mut self) {
        self.chunks.clear();
        self.statuses.clear();
        self.dirty_meshes.clear();
        self.edits.clear();
        self.pending_generation.clear();
        self.pending_meshing.clear();
        self.ready_meshes.clear();
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn dirty_count(&self) -> usize {
        self.dirty_meshes.len()
    }

    pub fn edit_count(&self) -> usize {
        self.edits.values().map(|m| m.len()).sum::<usize>() + self.delta_pager.total_saved_count()
    }

    pub fn status_of(&self, pos: IVec3) -> ChunkStatus {
        self.statuses.get(&pos).copied().unwrap_or(ChunkStatus::Empty)
    }

    pub fn get_all_raw_edits(&self) -> Vec<([i32; 3], u16)> {
        let mut list = Vec::new();
        for (&chunk_pos, chunk_edits) in &self.edits {
            let base = chunk_pos * CHUNK_SIZE_I32;
            for &(code, id) in &chunk_edits.edits {
                let (lx, ly, lz) = unpack_delta_index(code);
                let wp = base + IVec3::new(lx as i32, ly as i32, lz as i32);
                list.push(([wp.x, wp.y, wp.z], id.0));
            }
        }
        list
    }

    pub fn get_block(&self, pos: IVec3) -> BlockId {
        let (chunk_pos, [lx, ly, lz]) = world_to_chunk(pos);
        if let Some(block) = self.edits.get(&chunk_pos).and_then(|m| m.get(lx, ly, lz)) {
            return block;
        }
        if let Some(block) = self.delta_pager.peek(&chunk_pos).and_then(|m| m.get(lx, ly, lz)) {
            return block;
        }
        self.chunks
            .get(&chunk_pos)
            .map_or(BlockId::AIR, |chunk| chunk.get(lx, ly, lz))
    }

    pub fn set_block(&mut self, pos: IVec3, id: BlockId) {
        let (chunk_pos, [lx, ly, lz]) = world_to_chunk(pos);
        self.edits
            .entry(chunk_pos)
            .or_default()
            .set(lx, ly, lz, id);

        let chunk_arc = self.chunks.entry(chunk_pos).or_insert_with(|| {
            Arc::new(Chunk::empty())
        });
        let chunk = Arc::make_mut(chunk_arc);
        chunk.set(lx, ly, lz, id);

        self.statuses.insert(chunk_pos, ChunkStatus::Dirty);
        self.dirty_meshes.insert(chunk_pos);

        // Boundary neighbor detection
        if lx == 0 {
            let n = chunk_pos + IVec3::new(-1, 0, 0);
            if self.chunks.contains_key(&n) {
                self.statuses.insert(n, ChunkStatus::Dirty);
                self.dirty_meshes.insert(n);
            }
        } else if lx == CHUNK_SIZE - 1 {
            let n = chunk_pos + IVec3::new(1, 0, 0);
            if self.chunks.contains_key(&n) {
                self.statuses.insert(n, ChunkStatus::Dirty);
                self.dirty_meshes.insert(n);
            }
        }

        if ly == 0 {
            let n = chunk_pos + IVec3::new(0, -1, 0);
            if self.chunks.contains_key(&n) {
                self.statuses.insert(n, ChunkStatus::Dirty);
                self.dirty_meshes.insert(n);
            }
        } else if ly == CHUNK_SIZE - 1 {
            let n = chunk_pos + IVec3::new(0, 1, 0);
            if self.chunks.contains_key(&n) {
                self.statuses.insert(n, ChunkStatus::Dirty);
                self.dirty_meshes.insert(n);
            }
        }

        if lz == 0 {
            let n = chunk_pos + IVec3::new(0, 0, -1);
            if self.chunks.contains_key(&n) {
                self.statuses.insert(n, ChunkStatus::Dirty);
                self.dirty_meshes.insert(n);
            }
        } else if lz == CHUNK_SIZE - 1 {
            let n = chunk_pos + IVec3::new(0, 0, 1);
            if self.chunks.contains_key(&n) {
                self.statuses.insert(n, ChunkStatus::Dirty);
                self.dirty_meshes.insert(n);
            }
        }
    }

    /// Breaks a block and performs a structural integrity BFS check, returning any detached falling clusters.
    pub fn break_block_with_integrity(
        &mut self,
        pos: IVec3,
        registry: &BlockRegistry,
        materials: &MaterialRegistry,
        ground_anchor_y: i32,
    ) -> Vec<DetachedVoxelCluster> {
        self.set_block(pos, BlockId::AIR);
        StructuralIntegrityChecker::check_and_detach(self, pos, registry, materials, ground_anchor_y)
    }

    fn apply_edits_to_chunk(&mut self, chunk_pos: IVec3, chunk: &mut Chunk) {
        if let Some(chunk_edits) = self.edits.get(&chunk_pos) {
            for &(code, id) in &chunk_edits.edits {
                let (lx, ly, lz) = unpack_delta_index(code);
                chunk.set(lx, ly, lz, id);
            }
        } else if let Some(paged_delta) = self.delta_pager.page_in(chunk_pos) {
            for &(code, id) in &paged_delta.edits {
                let (lx, ly, lz) = unpack_delta_index(code);
                chunk.set(lx, ly, lz, id);
            }
            self.edits.insert(chunk_pos, paged_delta);
        }
    }

    pub fn save_edits(&self, path: &Path, registry: &BlockRegistry) -> io::Result<usize> {
        let mut records: Vec<(IVec3, &str)> = Vec::new();
        let mut seen = HashSet::new();

        for (&chunk_pos, chunk_edits) in &self.edits {
            seen.insert(chunk_pos);
            let base = chunk_pos * CHUNK_SIZE_I32;
            for &(code, id) in &chunk_edits.edits {
                let (lx, ly, lz) = unpack_delta_index(code);
                let name = registry
                    .get(id)
                    .map(|d| d.name.as_str())
                    .unwrap_or("core:air");
                records.push((base + IVec3::new(lx as i32, ly as i32, lz as i32), name));
            }
        }

        for (&chunk_pos, chunk_edits) in &self.delta_pager.cache {
            if seen.contains(&chunk_pos) {
                continue;
            }
            seen.insert(chunk_pos);
            let base = chunk_pos * CHUNK_SIZE_I32;
            for &(code, id) in &chunk_edits.edits {
                let (lx, ly, lz) = unpack_delta_index(code);
                let name = registry
                    .get(id)
                    .map(|d| d.name.as_str())
                    .unwrap_or("core:air");
                records.push((base + IVec3::new(lx as i32, ly as i32, lz as i32), name));
            }
        }

        for (&chunk_pos, chunk_edits) in &self.delta_pager.disk_storage {
            if seen.contains(&chunk_pos) {
                continue;
            }
            seen.insert(chunk_pos);
            let base = chunk_pos * CHUNK_SIZE_I32;
            for &(code, id) in &chunk_edits.edits {
                let (lx, ly, lz) = unpack_delta_index(code);
                let name = registry
                    .get(id)
                    .map(|d| d.name.as_str())
                    .unwrap_or("core:air");
                records.push((base + IVec3::new(lx as i32, ly as i32, lz as i32), name));
            }
        }

        let tmp_path = path.with_extension("dat.tmp");
        {
            let file = File::create(&tmp_path)?;
            let mut w = BufWriter::new(file);
            w.write_all(WORLD_MAGIC)?;
            w.write_all(&(records.len() as u64).to_le_bytes())?;
            for (pos, name) in &records {
                w.write_all(&pos.x.to_le_bytes())?;
                w.write_all(&pos.y.to_le_bytes())?;
                w.write_all(&pos.z.to_le_bytes())?;
                let bytes = name.as_bytes();
                let len = u16::try_from(bytes.len()).unwrap_or(u16::MAX);
                w.write_all(&len.to_le_bytes())?;
                w.write_all(&bytes[..len as usize])?;
            }
            w.flush()?;
        }
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
        std::fs::rename(&tmp_path, path)?;
        Ok(records.len())
    }

    pub fn load_edits(&mut self, path: &Path, registry: &BlockRegistry) -> io::Result<usize> {
        if !path.exists() {
            return Ok(0);
        }
        let file = File::open(path)?;
        let mut r = BufReader::new(file);
        let mut magic = [0u8; 4];
        r.read_exact(&mut magic)?;
        if &magic != WORLD_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported world save format",
            ));
        }

        let mut count_buf = [0u8; 8];
        r.read_exact(&mut count_buf)?;
        let count = u64::from_le_bytes(count_buf) as usize;

        let mut loaded = 0usize;
        for _ in 0..count {
            let mut xyz = [0u8; 12];
            r.read_exact(&mut xyz)?;
            let x = i32::from_le_bytes(xyz[0..4].try_into().unwrap());
            let y = i32::from_le_bytes(xyz[4..8].try_into().unwrap());
            let z = i32::from_le_bytes(xyz[8..12].try_into().unwrap());

            let mut len_buf = [0u8; 2];
            r.read_exact(&mut len_buf)?;
            let len = u16::from_le_bytes(len_buf) as usize;

            let mut name_bytes = vec![0u8; len];
            r.read_exact(&mut name_bytes)?;
            let name = String::from_utf8_lossy(&name_bytes);

            let Some(id) = registry.find(&name) else {
                continue;
            };
            let (chunk_pos, local) = world_to_chunk(IVec3::new(x, y, z));
            self.edits
                .entry(chunk_pos)
                .or_default()
                .set(local[0], local[1], local[2], id);
            loaded += 1;
        }
        Ok(loaded)
    }

    /// Neighbor-Aware Meshing Gate:
    /// A chunk can ONLY be dispatched for meshing if it and all 26 neighbors
    /// are in state >= DataReady (or outside world vertical bounds).
    #[inline]
    pub fn can_mesh(&self, pos: IVec3) -> bool {
        let Some(&status) = self.statuses.get(&pos) else {
            return false;
        };
        if status < ChunkStatus::DataReady {
            return false;
        }

        for dy in -1..=1 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 && dz == 0 {
                        continue;
                    }
                    let npos = pos + IVec3::new(dx, dy, dz);
                    if npos.y < self.min_chunk_y || npos.y > self.max_chunk_y {
                        continue;
                    }
                    match self.statuses.get(&npos) {
                        Some(&st) if st >= ChunkStatus::DataReady => {}
                        _ => return false,
                    }
                }
            }
        }
        true
    }

    /// Generates missing chunks synchronously (useful for initial spawn area).
    pub fn ensure_chunks_around(
        &mut self,
        center: IVec3,
        radius_xz: i32,
        generator: &dyn WorldGenerator,
    ) -> usize {
        let (min_y, max_y) = generator.vertical_bounds();
        self.min_chunk_y = min_y;
        self.max_chunk_y = max_y;

        let mut missing = Vec::new();
        for cz in (center.z - radius_xz)..=(center.z + radius_xz) {
            for cx in (center.x - radius_xz)..=(center.x + radius_xz) {
                let col_max_h = generator.column_max_height(cx * CHUNK_SIZE_I32 + 8, cz * CHUNK_SIZE_I32 + 8);
                let max_solid_cy = col_max_h.div_euclid(CHUNK_SIZE_I32);

                for cy in min_y..=max_y {
                    let pos = IVec3::new(cx, cy, cz);
                    if self.chunks.contains_key(&pos) {
                        continue;
                    }
                    if cy > max_solid_cy {
                        self.chunks.insert(pos, Arc::new(Chunk::empty()));
                        self.statuses.insert(pos, ChunkStatus::DataReady);
                        continue;
                    }
                    missing.push(pos);
                }
            }
        }

        if missing.is_empty() {
            return 0;
        }

        let generated: Vec<(IVec3, Chunk)> = missing
            .into_par_iter()
            .map(|pos| {
                let mut blocks = vec![BlockId::AIR; CHUNK_VOLUME];
                generator.generate_chunk(pos, CHUNK_SIZE, &mut blocks);
                (pos, Chunk::from_blocks(blocks))
            })
            .collect();

        let count = generated.len();
        for (pos, mut chunk) in generated {
            self.apply_edits_to_chunk(pos, &mut chunk);
            self.chunks.insert(pos, Arc::new(chunk));
            self.statuses.insert(pos, ChunkStatus::DataReady);
            self.dirty_meshes.insert(pos);
        }
        count
    }

    /// Enqueues missing chunks around `center` to the background generator without blocking.
    /// Uses dynamic vertical bounds and skips chunks guaranteed above mountain terrain.
    pub fn request_chunks_around(
        &mut self,
        center: IVec3,
        radius_xz: i32,
        generator: &dyn WorldGenerator,
        streamer: &WorldStreamer,
        max_enqueue: usize,
    ) {
        let (min_y, max_y) = generator.vertical_bounds();
        self.min_chunk_y = min_y;
        self.max_chunk_y = max_y;

        if self.pending_generation.len() >= max_enqueue * 2 {
            return;
        }

        let mut missing = Vec::new();
        for cz in (center.z - radius_xz)..=(center.z + radius_xz) {
            for cx in (center.x - radius_xz)..=(center.x + radius_xz) {
                let col_max_h = generator.column_max_height(cx * CHUNK_SIZE_I32 + 8, cz * CHUNK_SIZE_I32 + 8);
                let max_solid_cy = col_max_h.div_euclid(CHUNK_SIZE_I32);

                for cy in min_y..=max_y {
                    let pos = IVec3::new(cx, cy, cz);
                    if self.chunks.contains_key(&pos) || self.pending_generation.contains(&pos) {
                        continue;
                    }
                    if cy > max_solid_cy {
                        self.chunks.insert(pos, Arc::new(Chunk::empty()));
                        self.statuses.insert(pos, ChunkStatus::DataReady);
                        continue;
                    }
                    missing.push(pos);
                }
            }
        }

        if missing.is_empty() {
            return;
        }

        missing.sort_unstable_by_key(|p| {
            let dx = p.x - center.x;
            let dy = p.y - center.y;
            let dz = p.z - center.z;
            dx * dx + dy * dy + dz * dz
        });
        missing.truncate(max_enqueue);

        for &pos in &missing {
            self.pending_generation.insert(pos);
        }
        if let Some(tx) = &streamer.gen_tx {
            let _ = tx.send(missing);
        }
    }

    /// Enqueues dirty chunk meshes closest to `center` to the background worker.
    /// Strictly filters candidates through `can_mesh` (Neighbor-Aware Meshing Gate).
    pub fn dispatch_dirty_meshes(
        &mut self,
        center: IVec3,
        streamer: &WorldStreamer,
        max_enqueue: usize,
    ) {
        if self.dirty_meshes.is_empty() || self.pending_meshing.len() >= max_enqueue * 2 {
            return;
        }

        let mut candidates: Vec<IVec3> = self
            .dirty_meshes
            .iter()
            .copied()
            .filter(|pos| !self.pending_meshing.contains(pos) && self.can_mesh(*pos))
            .collect();

        if candidates.is_empty() {
            return;
        }

        candidates.sort_unstable_by_key(|p| {
            let dx = p.x - center.x;
            let dy = p.y - center.y;
            let dz = p.z - center.z;
            dx * dx + dy * dy + dz * dz
        });
        candidates.truncate(max_enqueue);

        let mut jobs = Vec::with_capacity(candidates.len());
        for pos in candidates {
            self.dirty_meshes.remove(&pos);
            if !self.chunks.contains_key(&pos) {
                continue;
            }
            self.pending_meshing.insert(pos);
            jobs.push((pos, self.neighborhood_arcs(pos)));
        }

        if !jobs.is_empty() {
            if let Some(tx) = &streamer.mesh_tx {
                let _ = tx.send(jobs);
            }
        }
    }

    /// Collects completed chunks & meshes from background workers.
    /// Sorts ready meshes by distance to `center` so nearby chunks upload first,
    /// and returns at most `max_upload` meshes per frame.
    pub fn poll_streamer(
        &mut self,
        streamer: &WorldStreamer,
        center: IVec3,
        max_upload: usize,
    ) -> Vec<(IVec3, ChunkMesh)> {
        while let Ok(resp) = streamer.resp_rx.try_recv() {
            match resp {
                WorkerResponse::Generated(generated) => {
                    for (pos, mut chunk) in generated {
                        self.pending_generation.remove(&pos);
                        self.apply_edits_to_chunk(pos, &mut chunk);
                        self.chunks.insert(pos, Arc::new(chunk));
                        self.statuses.insert(pos, ChunkStatus::DataReady);
                        self.dirty_meshes.insert(pos);

                        for dy in -1..=1 {
                            for dz in -1..=1 {
                                for dx in -1..=1 {
                                    if dx == 0 && dy == 0 && dz == 0 {
                                        continue;
                                    }
                                    let n = pos + IVec3::new(dx, dy, dz);
                                    if self.chunks.contains_key(&n) {
                                        self.dirty_meshes.insert(n);
                                    }
                                }
                            }
                        }
                    }
                }
                WorkerResponse::Meshed(built) => {
                    for (pos, mesh) in built {
                        self.pending_meshing.remove(&pos);
                        self.statuses.insert(pos, ChunkStatus::Meshed);
                        self.ready_meshes.push((pos, mesh));
                    }
                }
            }
        }

        if self.ready_meshes.is_empty() {
            return Vec::new();
        }

        // Distance-Priority Upload: sort ready meshes so closest to player upload first
        self.ready_meshes.sort_unstable_by_key(|(p, _)| {
            let dx = p.x - center.x;
            let dy = p.y - center.y;
            let dz = p.z - center.z;
            dx * dx + dy * dy + dz * dz
        });

        let take_count = self.ready_meshes.len().min(max_upload);
        self.ready_meshes.drain(0..take_count).collect()
    }

    /// Unloads chunks outside `radius_xz + 2` around `center` and prunes stale upload meshes.
    /// Pure procedural chunks are dropped immediately without disk baggage,
    /// while chunks with player edits are managed via the LRU delta pager.
    pub fn unload_outside(&mut self, center: IVec3, radius_xz: i32) -> Vec<IVec3> {
        let keep = radius_xz + 2;
        let to_remove: Vec<IVec3> = self
            .chunks
            .keys()
            .copied()
            .filter(|pos| (pos.x - center.x).abs() > keep || (pos.z - center.z).abs() > keep)
            .collect();

        if to_remove.is_empty() {
            return Vec::new();
        }

        let to_remove_set: HashSet<IVec3> = to_remove.iter().copied().collect();

        for pos in &to_remove {
            self.chunks.remove(pos);
            self.statuses.remove(pos);
            self.dirty_meshes.remove(pos);
            self.pending_generation.remove(pos);
            self.pending_meshing.remove(pos);

            // Clean procedural eviction & LRU paging
            if let Some(delta) = self.edits.remove(pos) {
                if !delta.is_empty() {
                    self.delta_pager.page_out(*pos, delta);
                }
            }
        }

        self.ready_meshes.retain(|(pos, _)| !to_remove_set.contains(pos));

        to_remove
    }

    pub fn neighborhood_arcs(&self, pos: IVec3) -> ChunkNeighborhood {
        let mut arr: ChunkNeighborhood = Default::default();
        for dy in -1..=1 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let idx = ((dx + 1) + (dz + 1) * 3 + (dy + 1) * 9) as usize;
                    arr[idx] = self.chunks.get(&(pos + IVec3::new(dx, dy, dz))).cloned();
                }
            }
        }
        arr
    }

    /// Synchronous fast-path: immediately rebuilds meshes for specified chunks in the current frame.
    /// Perfect for instant bullet destruction, block breaking, or explosions.
    pub fn rebuild_immediate_meshes(
        &mut self,
        chunks: &[IVec3],
        registry: &BlockRegistry,
    ) -> Vec<(IVec3, ChunkMesh)> {
        for &pos in chunks {
            self.dirty_meshes.remove(&pos);
            self.statuses.insert(pos, ChunkStatus::Meshed);
        }
        let scale = self.voxel_scale;
        let jobs: Vec<_> = chunks
            .iter()
            .copied()
            .filter(|pos| self.chunks.contains_key(pos))
            .map(|pos| (pos, self.neighborhood_arcs(pos)))
            .collect();

        jobs.into_par_iter()
            .map(|(pos, neighborhood)| {
                let mesh = build_chunk_mesh(pos, &neighborhood, registry, scale);
                (pos, mesh)
            })
            .collect()
    }

    /// Builds meshes synchronously for up to `max_chunks` dirty chunks.
    pub fn rebuild_dirty_meshes(
        &mut self,
        registry: &BlockRegistry,
        max_chunks: usize,
    ) -> Vec<(IVec3, ChunkMesh)> {
        if self.dirty_meshes.is_empty() || max_chunks == 0 {
            return Vec::new();
        }

        let batch: Vec<IVec3> = self
            .dirty_meshes
            .iter()
            .copied()
            .take(max_chunks)
            .collect();
        for pos in &batch {
            self.dirty_meshes.remove(pos);
        }

        let scale = self.voxel_scale;
        let jobs: Vec<_> = batch
            .into_iter()
            .filter(|pos| self.chunks.contains_key(pos))
            .map(|pos| (pos, self.neighborhood_arcs(pos)))
            .collect();

        jobs.into_par_iter()
            .map(|(pos, neighborhood)| {
                let mesh = build_chunk_mesh(pos, &neighborhood, registry, scale);
                (pos, mesh)
            })
            .collect()
    }

    /// Fast 3D DDA raycast against solid voxels, supporting custom `voxel_scale`.
    pub fn raycast(
        &self,
        origin: Vec3,
        dir: Vec3,
        max_dist: f32,
        registry: &BlockRegistry,
    ) -> Option<RaycastHit> {
        let dir = dir.normalize_or_zero();
        if dir == Vec3::ZERO {
            return None;
        }

        let scale = self.voxel_scale;
        let inv_scale = 1.0 / scale;
        let scaled_origin = origin * inv_scale;

        let mut voxel = scaled_origin.floor().as_ivec3();
        let step = IVec3::new(
            if dir.x >= 0.0 { 1 } else { -1 },
            if dir.y >= 0.0 { 1 } else { -1 },
            if dir.z >= 0.0 { 1 } else { -1 },
        );

        let inv = Vec3::new(
            if dir.x.abs() > 1e-6 { 1.0 / dir.x.abs() } else { f32::INFINITY },
            if dir.y.abs() > 1e-6 { 1.0 / dir.y.abs() } else { f32::INFINITY },
            if dir.z.abs() > 1e-6 { 1.0 / dir.z.abs() } else { f32::INFINITY },
        );

        let mut t_max = Vec3::new(
            if dir.x >= 0.0 {
                (voxel.x as f32 + 1.0 - scaled_origin.x) * inv.x
            } else {
                (scaled_origin.x - voxel.x as f32) * inv.x
            },
            if dir.y >= 0.0 {
                (voxel.y as f32 + 1.0 - scaled_origin.y) * inv.y
            } else {
                (scaled_origin.y - voxel.y as f32) * inv.y
            },
            if dir.z >= 0.0 {
                (voxel.z as f32 + 1.0 - scaled_origin.z) * inv.z
            } else {
                (scaled_origin.z - voxel.z as f32) * inv.z
            },
        );

        let mut last_voxel = voxel;
        let mut dist = 0.0;
        let max_dist_scaled = max_dist * inv_scale;

        while dist <= max_dist_scaled {
            let block = self.get_block(voxel);
            if registry.is_solid(block) {
                return Some(RaycastHit {
                    hit_pos: voxel,
                    place_pos: last_voxel,
                    block,
                });
            }

            last_voxel = voxel;
            if t_max.x < t_max.y && t_max.x < t_max.z {
                dist = t_max.x;
                t_max.x += inv.x;
                voxel.x += step.x;
            } else if t_max.y < t_max.z {
                dist = t_max.y;
                t_max.y += inv.y;
                voxel.y += step.y;
            } else {
                dist = t_max.z;
                t_max.z += inv.z;
                voxel.z += step.z;
            }
        }

        None
    }
}

#[inline]
fn get_block_in_neighborhood(
    neighborhood: &ChunkNeighborhood,
    lx: i32,
    ly: i32,
    lz: i32,
) -> BlockId {
    let cx = if lx < 0 { 0 } else if lx >= CHUNK_SIZE_I32 { 2 } else { 1 };
    let cy = if ly < 0 { 0 } else if ly >= CHUNK_SIZE_I32 { 2 } else { 1 };
    let cz = if lz < 0 { 0 } else if lz >= CHUNK_SIZE_I32 { 2 } else { 1 };

    let Some(chunk) = &neighborhood[cx + cz * 3 + cy * 9] else {
        return BlockId::AIR;
    };
    let ux = (lx & 15) as usize;
    let uy = (ly & 15) as usize;
    let uz = (lz & 15) as usize;
    chunk.get(ux, uy, uz)
}

#[inline]
fn is_opaque_in_neighborhood(
    neighborhood: &ChunkNeighborhood,
    registry: &BlockRegistry,
    lx: i32,
    ly: i32,
    lz: i32,
) -> bool {
    let cx = if lx < 0 {
        0
    } else if lx >= CHUNK_SIZE_I32 {
        2
    } else {
        1
    };
    let cy = if ly < 0 {
        0
    } else if ly >= CHUNK_SIZE_I32 {
        2
    } else {
        1
    };
    let cz = if lz < 0 {
        0
    } else if lz >= CHUNK_SIZE_I32 {
        2
    } else {
        1
    };

    let Some(chunk) = &neighborhood[cx + cz * 3 + cy * 9] else {
        return false;
    };
    let ux = (lx & 15) as usize;
    let uy = (ly & 15) as usize;
    let uz = (lz & 15) as usize;
    registry.is_opaque(chunk.get(ux, uy, uz))
}

#[inline]
fn corner_ao_level(
    neighborhood: &ChunkNeighborhood,
    registry: &BlockRegistry,
    face_voxel: [i32; 3],
    normal: [i32; 3],
    corner_mask: [i32; 3],
) -> u8 {
    let ox = if normal[0] != 0 { 0 } else if corner_mask[0] != 0 { 1 } else { -1 };
    let oy = if normal[1] != 0 { 0 } else if corner_mask[1] != 0 { 1 } else { -1 };
    let oz = if normal[2] != 0 { 0 } else if corner_mask[2] != 0 { 1 } else { -1 };

    let (s1, s2) = if normal[0] != 0 {
        ([0, oy, 0], [0, 0, oz])
    } else if normal[1] != 0 {
        ([ox, 0, 0], [0, 0, oz])
    } else {
        ([ox, 0, 0], [0, oy, 0])
    };

    let side1 = is_opaque_in_neighborhood(
        neighborhood,
        registry,
        face_voxel[0] + s1[0],
        face_voxel[1] + s1[1],
        face_voxel[2] + s1[2],
    );
    let side2 = is_opaque_in_neighborhood(
        neighborhood,
        registry,
        face_voxel[0] + s2[0],
        face_voxel[1] + s2[1],
        face_voxel[2] + s2[2],
    );
    let diag = is_opaque_in_neighborhood(
        neighborhood,
        registry,
        face_voxel[0] + ox,
        face_voxel[1] + oy,
        face_voxel[2] + oz,
    );

    if side1 && side2 {
        3
    } else {
        ((side1 as u8) + (side2 as u8) + (diag as u8)).min(3)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct GreedyFaceData {
    id: BlockId,
    ao_packed: u8,
}

/// Builds high-performance chunk mesh with 2D Greedy Meshing.
/// Adjacent coplanar faces with identical material and AO levels are merged into large quads.
/// UV coordinates scale with quad dimensions for seamless repeat in Texture2DArray.
pub fn build_chunk_mesh(
    chunk_pos: IVec3,
    neighborhood: &ChunkNeighborhood,
    registry: &BlockRegistry,
    voxel_scale: f32,
) -> ChunkMesh {
    let Some(chunk) = &neighborhood[13] else {
        return ChunkMesh::default();
    };
    if chunk.is_empty() {
        return ChunkMesh::default();
    }

    let mut mesh = ChunkMesh::default();
    let base_origin = (chunk_pos * CHUNK_SIZE_I32).as_vec3() * voxel_scale;

    // Face configurations:
    // (Face, normal_dir [nx, ny, nz], corner_masks)
    let faces: [(BlockFace, [i32; 3], [[i32; 3]; 4]); 6] = [
        (
            BlockFace::Top,
            [0, 1, 0],
            [[0, 1, 1], [1, 1, 1], [1, 1, 0], [0, 1, 0]],
        ),
        (
            BlockFace::Bottom,
            [0, -1, 0],
            [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]],
        ),
        (
            BlockFace::North,
            [0, 0, -1],
            [[1, 0, 0], [0, 0, 0], [0, 1, 0], [1, 1, 0]],
        ),
        (
            BlockFace::South,
            [0, 0, 1],
            [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]],
        ),
        (
            BlockFace::East,
            [1, 0, 0],
            [[1, 0, 1], [1, 0, 0], [1, 1, 0], [1, 1, 1]],
        ),
        (
            BlockFace::West,
            [-1, 0, 0],
            [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]],
        ),
    ];

    for (face, norm_i, corner_masks) in faces {
        let normal = [norm_i[0] as f32, norm_i[1] as f32, norm_i[2] as f32];

        // Slicing through chunk depth:
        // Top/Bottom slices along Y (u=X, v=Z)
        // North/South slices along Z (u=X, v=Y)
        // East/West slices along X (u=Z, v=Y)
        for slice in 0..CHUNK_SIZE {
            let mut mask = [[None; CHUNK_SIZE]; CHUNK_SIZE];

            for v in 0..CHUNK_SIZE {
                for u in 0..CHUNK_SIZE {
                    let (x, y, z) = match face {
                        BlockFace::Top | BlockFace::Bottom => (u, slice, v),
                        BlockFace::North | BlockFace::South => (u, v, slice),
                        BlockFace::East | BlockFace::West => (slice, v, u),
                    };

                    let id = chunk.get(x, y, z);
                    if id == BlockId::AIR {
                        continue;
                    }
                    let Some(def) = registry.get(id) else {
                        continue;
                    };
                    if !def.solid && def.shading_model != ShadingModel::Water {
                        continue;
                    }

                    let face_voxel = [x as i32 + norm_i[0], y as i32 + norm_i[1], z as i32 + norm_i[2]];
                    let neighbor_block = get_block_in_neighborhood(neighborhood, face_voxel[0], face_voxel[1], face_voxel[2]);
                    if id == neighbor_block && def.transparency != BlockTransparency::Opaque {
                        continue;
                    }
                    if is_opaque_in_neighborhood(neighborhood, registry, face_voxel[0], face_voxel[1], face_voxel[2]) {
                        continue;
                    }

                    // Pack the 4 corner AO levels into 1 byte (2 bits each)
                    let mut ao_packed = 0u8;
                    for (i, &cmask) in corner_masks.iter().enumerate() {
                        let lvl = corner_ao_level(neighborhood, registry, face_voxel, norm_i, cmask);
                        ao_packed |= (lvl & 3) << (i * 2);
                    }

                    mask[u][v] = Some(GreedyFaceData { id, ao_packed });
                }
            }

            // 2D Greedy Meshing pass on this slice
            for v in 0..CHUNK_SIZE {
                for u in 0..CHUNK_SIZE {
                    let Some(current) = mask[u][v] else {
                        continue;
                    };

                    let Some(def) = registry.get(current.id) else {
                        continue;
                    };

                    // For water, avoid merging quads so sine-wave vertex displacements animate smoothly per block
                    let is_water = def.shading_model == ShadingModel::Water;

                    // 1. Determine width (U-extent)
                    let mut width = 1;
                    if !is_water {
                        while u + width < CHUNK_SIZE && mask[u + width][v] == Some(current) {
                            width += 1;
                        }
                    }

                    // 2. Determine height (V-extent)
                    let mut height = 1;
                    if !is_water {
                        'v_loop: while v + height < CHUNK_SIZE {
                            for k in 0..width {
                                if mask[u + k][v + height] != Some(current) {
                                    break 'v_loop;
                                }
                            }
                            height += 1;
                        }
                    }

                    // 3. Clear merged cells from mask
                    for dv in 0..height {
                        for du in 0..width {
                            mask[u + du][v + dv] = None;
                        }
                    }

                    // 4. Emit merged quad
                    let color = def.face_colors[face as usize];
                    let tex_id = def.face_textures[face as usize] as u32;
                    let base_idx = mesh.vertices.len() as u32;

                    let ao0 = AO_CURVE[((current.ao_packed >> 0) & 3) as usize];
                    let ao1 = AO_CURVE[((current.ao_packed >> 2) & 3) as usize];
                    let ao2 = AO_CURVE[((current.ao_packed >> 4) & 3) as usize];
                    let ao3 = AO_CURVE[((current.ao_packed >> 6) & 3) as usize];

                    let w_f = width as f32;
                    let h_f = height as f32;

                    // Compute 4 corner positions in world space
                    let corners_local: [[f32; 3]; 4] = match face {
                        BlockFace::Top => [
                            [u as f32, (slice + 1) as f32, (v + height) as f32],
                            [(u + width) as f32, (slice + 1) as f32, (v + height) as f32],
                            [(u + width) as f32, (slice + 1) as f32, v as f32],
                            [u as f32, (slice + 1) as f32, v as f32],
                        ],
                        BlockFace::Bottom => [
                            [u as f32, slice as f32, v as f32],
                            [(u + width) as f32, slice as f32, v as f32],
                            [(u + width) as f32, slice as f32, (v + height) as f32],
                            [u as f32, slice as f32, (v + height) as f32],
                        ],
                        BlockFace::North => [
                            [(u + width) as f32, v as f32, slice as f32],
                            [u as f32, v as f32, slice as f32],
                            [u as f32, (v + height) as f32, slice as f32],
                            [(u + width) as f32, (v + height) as f32, slice as f32],
                        ],
                        BlockFace::South => [
                            [u as f32, v as f32, (slice + 1) as f32],
                            [(u + width) as f32, v as f32, (slice + 1) as f32],
                            [(u + width) as f32, (v + height) as f32, (slice + 1) as f32],
                            [u as f32, (v + height) as f32, (slice + 1) as f32],
                        ],
                        BlockFace::East => [
                            [(slice + 1) as f32, v as f32, (u + width) as f32],
                            [(slice + 1) as f32, v as f32, u as f32],
                            [(slice + 1) as f32, (v + height) as f32, u as f32],
                            [(slice + 1) as f32, (v + height) as f32, (u + width) as f32],
                        ],
                        BlockFace::West => [
                            [slice as f32, v as f32, u as f32],
                            [slice as f32, v as f32, (u + width) as f32],
                            [slice as f32, (v + height) as f32, (u + width) as f32],
                            [slice as f32, (v + height) as f32, u as f32],
                        ],
                    };

                    let uvs: [[f32; 2]; 4] = [
                        [0.0, h_f],
                        [w_f, h_f],
                        [w_f, 0.0],
                        [0.0, 0.0],
                    ];
                    let aos = [ao0, ao1, ao2, ao3];

                    for i in 0..4 {
                        mesh.vertices.push(VoxelVertex {
                            position: [
                                base_origin.x + corners_local[i][0] * voxel_scale,
                                base_origin.y + corners_local[i][1] * voxel_scale,
                                base_origin.z + corners_local[i][2] * voxel_scale,
                            ],
                            normal,
                            color,
                            ao: aos[i],
                            uv: uvs[i],
                            tex_id,
                            material_params: [
                                def.roughness,
                                def.metallic,
                                def.shading_model as u32 as f32,
                                def.subsurface,
                            ],
                        });
                    }

                    // Flip quad triangulation if diagonal AO gradient would crease
                    if aos[0] + aos[2] < aos[1] + aos[3] {
                        mesh.indices.extend_from_slice(&[
                            base_idx + 1,
                            base_idx + 2,
                            base_idx + 3,
                            base_idx + 1,
                            base_idx + 3,
                            base_idx,
                        ]);
                    } else {
                        mesh.indices.extend_from_slice(&[
                            base_idx,
                            base_idx + 1,
                            base_idx + 2,
                            base_idx,
                            base_idx + 2,
                            base_idx + 3,
                        ]);
                    }
                }
            }
        }
    }

    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::BlockDef;

    #[test]
    fn ao_darkens_adjacent_corners() {
        let mut reg = BlockRegistry::new();
        let stone = reg.register(BlockDef::solid_tinted(
            "test:stone",
            [0.5, 0.5, 0.5],
            [0.5, 0.5, 0.5],
            [0.5, 0.5, 0.5],
        ));

        let mut world = World::new();
        world.set_block(IVec3::new(5, 5, 5), stone);
        world.set_block(IVec3::new(4, 6, 5), stone);

        let meshes = world.rebuild_dirty_meshes(&reg, 8);
        assert!(!meshes.is_empty());
        let has_shaded_vertex = meshes
            .iter()
            .flat_map(|(_, m)| m.vertices.iter())
            .any(|v| v.ao < 0.99);
        assert!(has_shaded_vertex);
    }

    #[test]
    fn save_and_load_edits_roundtrip() {
        let mut reg = BlockRegistry::new();
        let wood = reg.register(BlockDef::solid_tinted(
            "test:wood",
            [0.5, 0.3, 0.2],
            [0.5, 0.3, 0.2],
            [0.5, 0.3, 0.2],
        ));

        let path = std::env::temp_dir().join("voxel_engine_world_test.dat");
        let _ = std::fs::remove_file(&path);

        let mut w1 = World::new();
        w1.set_block(IVec3::new(-3, 12, 40), wood);
        w1.set_block(IVec3::new(0, 0, 0), BlockId::AIR);
        let saved = w1.save_edits(&path, &reg).unwrap();
        assert_eq!(saved, 2);

        let mut w2 = World::new();
        let loaded = w2.load_edits(&path, &reg).unwrap();
        assert_eq!(loaded, 2);
        assert_eq!(w2.get_block(IVec3::new(-3, 12, 40)), wood);
        assert_eq!(w2.get_block(IVec3::new(0, 0, 0)), BlockId::AIR);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn greedy_meshing_merges_coplanar_quads() {
        let mut reg = BlockRegistry::new();
        let grass = reg.register(BlockDef::solid_tinted(
            "test:grass",
            [0.4, 0.8, 0.3],
            [0.4, 0.8, 0.3],
            [0.4, 0.8, 0.3],
        ));

        let mut world = World::new();
        for z in 0..4 {
            for x in 0..4 {
                world.set_block(IVec3::new(x, 0, z), grass);
            }
        }

        let meshes = world.rebuild_dirty_meshes(&reg, 1);
        assert!(!meshes.is_empty());
        let top_faces: Vec<_> = meshes[0].1.vertices.iter().filter(|v| v.normal[1] > 0.9).collect();
        assert_eq!(top_faces.len(), 4);
    }

    #[test]
    fn spatial_hash_and_infinite_coord_folding() {
        let (s1, l1) = sample_infinite_coord(0);
        assert_eq!((s1, l1), (0, 0));

        let (s2, l2) = sample_infinite_coord(INFINITE_PERIOD * 3 + 42);
        assert_eq!((s2, l2), (3, 42));

        let (s3, l3) = sample_infinite_coord(-50);
        assert_eq!(s3, -1);
        assert_eq!(l3, (INFINITE_PERIOD - 50) as i32);

        let h1 = spatial_hash_i64(10, 20, 30, 1337);
        let h2 = spatial_hash_i64(10, 20, 30, 1337);
        let h3 = spatial_hash_i64(11, 20, 30, 1337);
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn delta_pager_lru_eviction() {
        let mut pager = InfiniteDeltaPager::new(2);

        let mut d1 = ChunkDelta::new();
        d1.set(0, 0, 0, BlockId(1));
        let mut d2 = ChunkDelta::new();
        d2.set(1, 1, 1, BlockId(2));
        let mut d3 = ChunkDelta::new();
        d3.set(2, 2, 2, BlockId(3));

        pager.page_out(IVec3::new(0, 0, 0), d1);
        pager.page_out(IVec3::new(1, 0, 0), d2);
        pager.page_out(IVec3::new(2, 0, 0), d3);

        assert_eq!(pager.total_saved_count(), 3);
        assert!(pager.disk_storage.contains_key(&IVec3::new(0, 0, 0)));

        let restored = pager.page_in(IVec3::new(0, 0, 0)).expect("should restore from disk storage");
        assert_eq!(restored.get(0, 0, 0), Some(BlockId(1)));
    }

    #[test]
    fn structural_integrity_bfs_detaches_floating_voxels() {
        let mut reg = BlockRegistry::new();
        let stone = reg.register(BlockDef::solid_tinted(
            "test:stone",
            [0.5, 0.5, 0.5],
            [0.5, 0.5, 0.5],
            [0.5, 0.5, 0.5],
        ));
        let mat_reg = MaterialRegistry::new();

        let mut world = World::new();
        // Create a vertical pillar of stone from ground Y=0 to Y=3
        world.set_block(IVec3::new(0, 0, 0), stone);
        world.set_block(IVec3::new(0, 1, 0), stone);
        world.set_block(IVec3::new(0, 2, 0), stone);
        world.set_block(IVec3::new(0, 3, 0), stone);

        // Break support at Y=1. Y=2 and Y=3 should become detached!
        let detached = world.break_block_with_integrity(IVec3::new(0, 1, 0), &reg, &mat_reg, 0);

        assert_eq!(detached.len(), 1);
        assert_eq!(detached[0].blocks.len(), 2); // blocks at Y=2 and Y=3
        assert_eq!(world.get_block(IVec3::new(0, 2, 0)), BlockId::AIR);
        assert_eq!(world.get_block(IVec3::new(0, 3, 0)), BlockId::AIR);
    }

    #[test]
    fn voxel_contraption_greedy_colliders() {
        let mut contraption = VoxelContraption::new(
            1,
            RelativeTransform::default(),
            1.0,
        );

        // Build a 2x2x2 solid cube
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    contraption.set_block(IVec3::new(x, y, z), BlockId(1));
                }
            }
        }

        contraption.rebuild_colliders();
        // A solid 2x2x2 cube should merge into exactly 1 bounding box!
        assert_eq!(contraption.colliders.len(), 1);
        assert_eq!(contraption.colliders[0].min, Vec3::ZERO);
        assert_eq!(contraption.colliders[0].max, Vec3::new(2.0, 2.0, 2.0));
    }
}
