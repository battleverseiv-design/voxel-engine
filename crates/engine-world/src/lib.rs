use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use bytemuck::{Pod, Zeroable};
use engine_core::{BlockFace, BlockId, BlockRegistry, BlockTransparency, ShadingModel, WorldGenerator};
use glam::{IVec3, Vec3};
use rayon::prelude::*;

pub const CHUNK_SIZE: usize = 16;
pub const CHUNK_SIZE_I32: i32 = CHUNK_SIZE as i32;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE; // 4096
const WORLD_MAGIC: &[u8; 4] = b"VXW1";
const AO_CURVE: [f32; 4] = [1.0, 0.82, 0.66, 0.48];

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
        self.edits.values().map(|m| m.len()).sum()
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
        if chunk.get(lx, ly, lz) == id {
            return;
        }
        chunk.set(lx, ly, lz, id);

        self.statuses.insert(chunk_pos, ChunkStatus::Dirty);
        self.dirty_meshes.insert(chunk_pos);

        // Boundary neighbor detection: if block is on border, mark adjacent chunk Dirty
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

    fn apply_edits_to_chunk(&self, chunk_pos: IVec3, chunk: &mut Chunk) {
        let Some(chunk_edits) = self.edits.get(&chunk_pos) else {
            return;
        };
        for &(code, id) in &chunk_edits.edits {
            let (lx, ly, lz) = unpack_delta_index(code);
            chunk.set(lx, ly, lz, id);
        }
    }

    pub fn save_edits(&self, path: &Path, registry: &BlockRegistry) -> io::Result<usize> {
        let mut records: Vec<(IVec3, &str)> = Vec::new();
        for (&chunk_pos, chunk_edits) in &self.edits {
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

                        // Mark neighbors dirty so they can be meshed with this newly available neighbor
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

        // Distance-Priority Upload: sort ready meshes so closest to player upload first!
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
        }

        // Prune stale meshes waiting in upload queue
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
        world.set_block(IVec3::new(4, 6, 5), stone); // side neighbor above-west

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
        // Fill a 4x4 flat plane of grass
        for z in 0..4 {
            for x in 0..4 {
                world.set_block(IVec3::new(x, 0, z), grass);
            }
        }

        let meshes = world.rebuild_dirty_meshes(&reg, 1);
        assert!(!meshes.is_empty());
        let top_faces: Vec<_> = meshes[0].1.vertices.iter().filter(|v| v.normal[1] > 0.9).collect();
        // A merged 4x4 top face should emit only 4 vertices (1 quad) instead of 16 quads (64 vertices)!
        assert_eq!(top_faces.len(), 4);
    }
}
