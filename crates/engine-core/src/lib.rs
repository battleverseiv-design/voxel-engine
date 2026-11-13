pub mod inventory;
pub mod materials;
pub use inventory::*;
pub use materials::*;

use std::collections::HashMap;
use std::sync::Arc;

use glam::{IVec3, Quat, Vec3};

/// Unique identifier for a coordinate reference frame (e.g. planetary body, rotating ship, space station).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct ReferenceFrameId(pub u64);

impl ReferenceFrameId {
    /// The universal global inertial origin.
    pub const UNIVERSE_ROOT: Self = Self(0);
}

/// Global sector grid cell size in meters (256.0m).
pub const SECTOR_SIZE: f32 = 256.0;

/// Hierarchical position and orientation representation for massive/infinite worlds and moving reference frames.
/// Sector grid spacing is 256.0 meters, preserving single-precision `f32` floating point accuracy indefinitely.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RelativeTransform {
    pub parent_frame: ReferenceFrameId,
    pub sector: IVec3,
    pub local_offset: Vec3,
    pub local_rotation: Quat,
    pub local_velocity: Vec3,
}

impl Default for RelativeTransform {
    fn default() -> Self {
        Self {
            parent_frame: ReferenceFrameId::UNIVERSE_ROOT,
            sector: IVec3::ZERO,
            local_offset: Vec3::ZERO,
            local_rotation: Quat::IDENTITY,
            local_velocity: Vec3::ZERO,
        }
    }
}

impl RelativeTransform {
    pub fn new(parent_frame: ReferenceFrameId, sector: IVec3, local_offset: Vec3) -> Self {
        let mut t = Self {
            parent_frame,
            sector,
            local_offset,
            local_rotation: Quat::IDENTITY,
            local_velocity: Vec3::ZERO,
        };
        t.normalize();
        t
    }

    #[inline]
    pub fn world_position(&self) -> Vec3 {
        self.sector.as_vec3() * SECTOR_SIZE + self.local_offset
    }

    #[inline]
    pub fn set_world_position(&mut self, pos: Vec3) {
        self.sector = IVec3::ZERO;
        self.local_offset = pos;
        self.normalize();
    }

    /// Normalizes `local_offset` mathematically so that each axis coordinate lies strictly within `[0.0, SECTOR_SIZE)`,
    /// adjusting `sector` using euclidean division (`div_euclid` / `rem_euclid`).
    pub fn normalize(&mut self) {
        let sx = (self.local_offset.x / SECTOR_SIZE).floor() as i32;
        let sy = (self.local_offset.y / SECTOR_SIZE).floor() as i32;
        let sz = (self.local_offset.z / SECTOR_SIZE).floor() as i32;

        self.sector += IVec3::new(sx, sy, sz);
        self.local_offset.x -= sx as f32 * SECTOR_SIZE;
        self.local_offset.y -= sy as f32 * SECTOR_SIZE;
        self.local_offset.z -= sz as f32 * SECTOR_SIZE;

        // Ensure no negative floating point zero or rounding overshoot
        if self.local_offset.x < 0.0 {
            self.sector.x -= 1;
            self.local_offset.x += SECTOR_SIZE;
        } else if self.local_offset.x >= SECTOR_SIZE {
            self.sector.x += 1;
            self.local_offset.x -= SECTOR_SIZE;
        }

        if self.local_offset.y < 0.0 {
            self.sector.y -= 1;
            self.local_offset.y += SECTOR_SIZE;
        } else if self.local_offset.y >= SECTOR_SIZE {
            self.sector.y += 1;
            self.local_offset.y -= SECTOR_SIZE;
        }

        if self.local_offset.z < 0.0 {
            self.sector.z -= 1;
            self.local_offset.z += SECTOR_SIZE;
        } else if self.local_offset.z >= SECTOR_SIZE {
            self.sector.z += 1;
            self.local_offset.z -= SECTOR_SIZE;
        }

        self.local_rotation = self.local_rotation.normalize();
    }

    /// Converts this relative transform into continuous relative coordinates relative to a base sector in the same frame.
    #[inline]
    pub fn to_continuous(&self, base_sector: IVec3) -> Vec3 {
        let d_sector = self.sector - base_sector;
        Vec3::new(
            d_sector.x as f32 * SECTOR_SIZE + self.local_offset.x,
            d_sector.y as f32 * SECTOR_SIZE + self.local_offset.y,
            d_sector.z as f32 * SECTOR_SIZE + self.local_offset.z,
        )
    }

    /// Constructs a transform from continuous coordinates relative to a base sector in a frame.
    pub fn from_continuous(parent_frame: ReferenceFrameId, base_sector: IVec3, offset: Vec3) -> Self {
        let mut t = Self {
            parent_frame,
            sector: base_sector,
            local_offset: offset,
            local_rotation: Quat::IDENTITY,
            local_velocity: Vec3::ZERO,
        };
        t.normalize();
        t
    }

    /// Seamlessly reparents this transform to a new coordinate reference frame (e.g., when a player
    /// boards or steps off a moving spaceship onto a planetary body).
    /// `old_frame_in_new` is the position, orientation, and linear velocity of the old frame relative to `new_frame`.
    /// `old_frame_angular_velocity` is the angular rotation velocity vector of the old frame in radians per second.
    pub fn reparent_to(
        &mut self,
        new_frame: ReferenceFrameId,
        old_frame_in_new: &RelativeTransform,
        old_frame_angular_velocity: Vec3,
    ) {
        let local_pos = self.to_continuous(IVec3::ZERO);
        let rotated_pos = old_frame_in_new.local_rotation * local_pos;

        self.parent_frame = new_frame;
        self.sector = old_frame_in_new.sector;
        self.local_offset = old_frame_in_new.local_offset + rotated_pos;
        self.local_rotation = (old_frame_in_new.local_rotation * self.local_rotation).normalize();

        // v_new = v_frame + R * v_local + omega x r
        let tangential_velocity = old_frame_angular_velocity.cross(rotated_pos);
        self.local_velocity = old_frame_in_new.local_velocity
            + old_frame_in_new.local_rotation * self.local_velocity
            + tangential_velocity;

        self.normalize();
    }

    /// Simplified reparenting for non-rotating or static reference frames.
    pub fn reparent_simple(
        &mut self,
        new_frame: ReferenceFrameId,
        old_frame_in_new: &RelativeTransform,
    ) {
        self.reparent_to(new_frame, old_frame_in_new, Vec3::ZERO);
    }
}

/// Compact identifier for a voxel block type registered by a game mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BlockId(pub u16);

impl BlockId {
    /// Reserved empty space block always registered at index 0.
    pub const AIR: Self = Self(0);
    pub const WATER: Self = Self(13);
}

/// Six axis-aligned faces of a voxel cube.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum BlockFace {
    Top = 0,
    Bottom = 1,
    North = 2,
    South = 3,
    East = 4,
    West = 5,
}

/// Transparency classification for a voxel block face/mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlockTransparency {
    #[default]
    Opaque,
    Cutout,
    Translucent,
}

/// Data-driven definition of a block type.
#[derive(Debug, Clone)]
pub struct BlockDef {
    pub name: String,
    pub solid: bool,
    pub opaque: bool,
    /// RGB color tint per face (`[Top, Bottom, North, South, East, West]`).
    pub face_colors: [[f32; 3]; 6],
    /// Texture array index per face (`[Top, Bottom, North, South, East, West]`).
    /// 0 means untextured (vertex color only), 1..=N indexes texture array slice `tex_id - 1`.
    pub face_textures: [u16; 6],
    /// Material ID assigned per face (`[Top, Bottom, North, South, East, West]`).
    pub face_materials: [MaterialId; 6],
    /// Shading model (StandardPBR, Stylized, Foliage, Emissive).
    pub shading_model: ShadingModel,
    /// Foliage subsurface transmission strength (0.0 to 1.0).
    pub subsurface: f32,
    /// RGB emission color and strength (e.g. `[1.0, 0.85, 0.4]` for warm lamp). `[0.0, 0.0, 0.0]` for inert.
    pub emission: [f32; 3],
    /// Surface roughness from 0.0 (smooth specular) to 1.0 (rough diffuse).
    pub roughness: f32,
    /// Metallic factor from 0.0 (dielectric) to 1.0 (conductor).
    pub metallic: f32,
    /// Transparency mode.
    pub transparency: BlockTransparency,
    /// Mining hardness (determines time to break in seconds = hardness * 0.8).
    pub hardness: f32,
    /// Distinct RGB icon color for GUI / inventory slots.
    pub icon_color: [u8; 3],
}

impl BlockDef {
    /// Creates an opaque solid block with distinct top/side/bottom colors.
    pub fn solid_tinted(
        name: impl Into<String>,
        top: [f32; 3],
        side: [f32; 3],
        bottom: [f32; 3],
    ) -> Self {
        let icon_r = (top[0] * 255.0).clamp(0.0, 255.0) as u8;
        let icon_g = (top[1] * 255.0).clamp(0.0, 255.0) as u8;
        let icon_b = (top[2] * 255.0).clamp(0.0, 255.0) as u8;
        Self {
            name: name.into(),
            solid: true,
            opaque: true,
            face_colors: [top, bottom, side, side, side, side],
            face_textures: [0; 6],
            face_materials: [MaterialId::DEFAULT; 6],
            shading_model: ShadingModel::StandardPbr,
            subsurface: 0.0,
            emission: [0.0, 0.0, 0.0],
            roughness: 0.85,
            metallic: 0.0,
            transparency: BlockTransparency::Opaque,
            hardness: 1.0,
            icon_color: [icon_r, icon_g, icon_b],
        }
    }

    pub fn with_icon_color(mut self, color: [u8; 3]) -> Self {
        self.icon_color = color;
        self
    }

    pub fn with_hardness(mut self, hardness: f32) -> Self {
        self.hardness = hardness.max(0.05);
        self
    }

    pub fn with_shading(mut self, model: ShadingModel) -> Self {
        self.shading_model = model;
        self
    }

    pub fn with_subsurface(mut self, sss: f32) -> Self {
        self.subsurface = sss.clamp(0.0, 1.0);
        self
    }

    /// Applies full material parameters to this block definition.
    pub fn apply_material(mut self, mat: &MaterialDef) -> Self {
        self.shading_model = mat.shading_model;
        self.roughness = mat.roughness;
        self.metallic = mat.metallic;
        self.emission = mat.emission;
        self.subsurface = mat.subsurface;
        if mat.texture_id > 0 {
            self.face_textures = [mat.texture_id; 6];
        }
        self.face_colors = [[mat.base_color[0], mat.base_color[1], mat.base_color[2]]; 6];
        match mat.alpha_mode {
            AlphaMode::Opaque => self.transparency = BlockTransparency::Opaque,
            AlphaMode::Cutout => self.transparency = BlockTransparency::Cutout,
            AlphaMode::Blend => self.transparency = BlockTransparency::Translucent,
        }
        self.opaque = mat.alpha_mode == AlphaMode::Opaque;
        self
    }

    pub fn with_material(mut self, mat: MaterialId) -> Self {
        self.face_materials = [mat; 6];
        self
    }

    pub fn with_face_materials(
        mut self,
        top: MaterialId,
        side: MaterialId,
        bottom: MaterialId,
    ) -> Self {
        self.face_materials = [top, bottom, side, side, side, side];
        self
    }

    pub fn with_custom_face_materials(mut self, materials: [MaterialId; 6]) -> Self {
        self.face_materials = materials;
        self
    }

    pub fn with_texture(mut self, tex_id: u16) -> Self {
        self.face_textures = [tex_id; 6];
        self
    }

    pub fn with_face_textures(mut self, top: u16, side: u16, bottom: u16) -> Self {
        self.face_textures = [top, bottom, side, side, side, side];
        self
    }

    pub fn with_custom_face_textures(mut self, textures: [u16; 6]) -> Self {
        self.face_textures = textures;
        self
    }

    pub fn with_emission(mut self, emission: [f32; 3]) -> Self {
        self.emission = emission;
        self
    }

    pub fn with_roughness(mut self, roughness: f32) -> Self {
        self.roughness = roughness.clamp(0.0, 1.0);
        self
    }

    pub fn with_metallic(mut self, metallic: f32) -> Self {
        self.metallic = metallic.clamp(0.0, 1.0);
        self
    }

    pub fn with_transparency(mut self, transparency: BlockTransparency) -> Self {
        self.transparency = transparency;
        self.opaque = transparency == BlockTransparency::Opaque;
        self
    }

    #[inline]
    pub fn is_emissive(&self) -> bool {
        self.emission[0] > 0.001 || self.emission[1] > 0.001 || self.emission[2] > 0.001
    }
}

/// Runtime registry mapping `BlockId` to block metadata.
#[derive(Debug, Clone)]
pub struct BlockRegistry {
    defs: Vec<BlockDef>,
    by_name: HashMap<String, BlockId>,
}

impl Default for BlockRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            defs: Vec::new(),
            by_name: HashMap::new(),
        };
        registry.register(BlockDef {
            name: "core:air".to_string(),
            solid: false,
            opaque: false,
            face_colors: [[0.0; 3]; 6],
            face_textures: [0; 6],
            face_materials: [MaterialId::DEFAULT; 6],
            shading_model: ShadingModel::StandardPbr,
            subsurface: 0.0,
            emission: [0.0, 0.0, 0.0],
            roughness: 1.0,
            metallic: 0.0,
            transparency: BlockTransparency::Translucent,
            hardness: 0.0,
            icon_color: [0, 0, 0],
        });
        registry
    }

    pub fn register(&mut self, def: BlockDef) -> BlockId {
        if let Some(&existing) = self.by_name.get(&def.name) {
            return existing;
        }
        let id = BlockId(
            u16::try_from(self.defs.len()).expect("block registry exceeded u16::MAX entries"),
        );
        self.by_name.insert(def.name.clone(), id);
        self.defs.push(def);
        id
    }

    #[inline]
    pub fn get(&self, id: BlockId) -> Option<&BlockDef> {
        self.defs.get(id.0 as usize)
    }

    pub fn find(&self, name: &str) -> Option<BlockId> {
        self.by_name.get(name).copied()
    }

    /// Finds a block by exact name or suffix (e.g. `"stone"` matches `"survival:stone"`).
    pub fn find_fuzzy(&self, query: &str) -> Option<BlockId> {
        if let Some(exact) = self.find(query) {
            return Some(exact);
        }
        let q = query.to_ascii_lowercase();
        self.defs
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, def)| {
                let lower = def.name.to_ascii_lowercase();
                lower == q || lower.ends_with(&format!(":{q}"))
            })
            .map(|(idx, _)| BlockId(idx as u16))
    }

    /// Returns all registered non-air solid blocks `(BlockId, &BlockDef)`.
    pub fn solid_blocks(&self) -> Vec<(BlockId, &BlockDef)> {
        self.defs
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, def)| def.solid)
            .map(|(idx, def)| (BlockId(idx as u16), def))
            .collect()
    }

    #[inline]
    pub fn is_opaque(&self, id: BlockId) -> bool {
        self.get(id).is_some_and(|d| d.opaque)
    }

    #[inline]
    pub fn is_solid(&self, id: BlockId) -> bool {
        self.get(id).is_some_and(|d| d.solid)
    }
}

/// Core player gameplay mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerMode {
    Survival,
    Creative,
}

impl PlayerMode {
    pub fn label_ru(self) -> &'static str {
        match self {
            Self::Survival => "Выживание",
            Self::Creative => "Креатив",
        }
    }
}

/// Host context exposed by the runtime to slash commands.
pub trait CommandHost {
    fn player_position(&self) -> Vec3;
    fn teleport_player(&mut self, pos: Vec3);
    fn player_mode(&self) -> PlayerMode;
    fn set_player_mode(&mut self, mode: PlayerMode);
    fn is_flying(&self) -> bool;
    fn set_flying(&mut self, flying: bool);
    fn can_fly(&self) -> bool;
    fn set_can_fly(&mut self, allowed: bool);
    fn block_registry(&self) -> &BlockRegistry;
    fn set_selected_block(&mut self, id: BlockId);
    fn clear_chat(&mut self);
    fn save_world(&mut self) -> Result<usize, String>;
    fn time_of_day(&self) -> f32;
    fn set_time_of_day(&mut self, time: f32);
    fn material_registry(&self) -> &MaterialRegistry;
    fn heal_player(&mut self, _amount: f32) {}
    fn find_player(&self, _query: &str) -> Option<(String, Vec3)> { None }
}

type CommandFn = Box<dyn Fn(&[&str], &mut dyn CommandHost) -> Result<String, String> + Send + Sync>;

#[allow(dead_code)]
struct CommandEntry {
    name: &'static str,
    usage: &'static str,
    description: &'static str,
    handler: CommandFn,
}

/// Extensible slash-command registry (`/command args...`).
pub struct CommandRegistry {
    entries: Vec<CommandEntry>,
    lookup: HashMap<String, usize>,
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            lookup: HashMap::new(),
        }
    }

    pub fn register<F>(
        &mut self,
        name: &'static str,
        aliases: &[&'static str],
        usage: &'static str,
        description: &'static str,
        handler: F,
    ) where
        F: Fn(&[&str], &mut dyn CommandHost) -> Result<String, String> + Send + Sync + 'static,
    {
        let idx = self.entries.len();
        self.entries.push(CommandEntry {
            name,
            usage,
            description,
            handler: Box::new(handler),
        });
        self.lookup.insert(name.to_ascii_lowercase(), idx);
        for &alias in aliases {
            self.lookup.insert(alias.to_ascii_lowercase(), idx);
        }
    }

    pub fn help_lines(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|e| format!("{} — {}", e.usage, e.description))
            .collect()
    }

    pub fn execute(&self, input: &str, host: &mut dyn CommandHost) -> Result<String, String> {
        let trimmed = input.trim().trim_start_matches('/');
        if trimmed.is_empty() {
            return Err("Введите команду. Список: /help".to_string());
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        let cmd = parts[0].to_ascii_lowercase();
        let args = &parts[1..];

        if cmd == "help" || cmd == "?" {
            return Ok(format!("Команды:\n{}", self.help_lines().join("\n")));
        }

        let Some(&idx) = self.lookup.get(&cmd) else {
            return Err(format!("Неизвестная команда '/{cmd}'. Введите /help"));
        };
        (self.entries[idx].handler)(args, host)
    }
}

/// Optimization hint describing the structural density of a chunk volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeHint {
    /// The entire chunk consists exclusively of air/empty space.
    AllEmpty,
    /// The entire chunk is uniformly packed with a single solid block.
    AllSolid(BlockId),
    /// The chunk contains terrain surface, caves, structures, or mixed blocks.
    MixedSurface,
}

/// Thread-safe world generator implemented by a game mode.
pub trait WorldGenerator: Send + Sync {
    /// Fills a `chunk_size^3` slice in `x + z * size + y * size * size` order.
    fn generate_chunk(&self, chunk_pos: IVec3, chunk_size: usize, out: &mut [BlockId]);

    /// (min_chunk_y, max_chunk_y) inclusive range for this generator. Defaults to (-3, 6).
    fn vertical_bounds(&self) -> (i32, i32) {
        (-3, 6)
    }

    /// Maximum surface block height at world coordinate (x, z). Chunks with `base_y > column_max_height`
    /// that contain no other structures can be skipped or treated as air.
    fn column_max_height(&self, _x: i32, _z: i32) -> i32 {
        i32::MAX
    }

    /// Fast volumetric density hint for a chunk. Allows the streamer to bypass voxel generation
    /// or meshing for guaranteed uniform or empty chunks without sampling noise.
    fn query_volume_hint(&self, chunk_pos: IVec3) -> VolumeHint {
        let (min_y, max_y) = self.vertical_bounds();
        if chunk_pos.y < min_y || chunk_pos.y > max_y {
            return VolumeHint::AllEmpty;
        }

        let wx = chunk_pos.x * 16;
        let wz = chunk_pos.z * 16;
        let max_h = self.column_max_height(wx, wz);
        if chunk_pos.y * 16 > max_h {
            return VolumeHint::AllEmpty;
        }

        VolumeHint::MixedSurface
    }
}

/// Network synchronization visibility policy for voxel and entity updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisibilityPolicy {
    /// Broadcast all world updates within the player's full chunk render radius (classic Minecraft/survival).
    #[default]
    FullRadius,
    /// Occlusion-culling visibility: blocks and players hidden behind solid walls are not sent (anti-cheat for shooters/CS/DayZ).
    StrictLineOfSight,
}

/// Global world simulation and physics rules for a game mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimulationRules {
    /// Gravitational acceleration vector in m/s^2. Defaults to Earth-like `Vec3::new(0.0, -9.81, 0.0)`.
    pub gravity: Vec3,
    /// Physical voxel size in meters. Defaults to 1.0 (1 meter cubes), scalable to 0.5 or 0.25 for micro-voxels.
    pub voxel_scale: f32,
    /// Network visibility policy.
    pub visibility_policy: VisibilityPolicy,
    /// Structural integrity and collapse calculation flag (stress propagation / gravity collapse).
    pub structural_integrity: bool,
}

impl Default for SimulationRules {
    fn default() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            voxel_scale: 1.0,
            visibility_policy: VisibilityPolicy::FullRadius,
            structural_integrity: false,
        }
    }
}

/// Entry interface for pluggable game modes running on top of the engine.
pub trait GameMode: Send + Sync {
    fn id(&self) -> &'static str;
    fn register_blocks(&self, registry: &mut BlockRegistry);

    /// Optional hook to register game-specific materials into the material system.
    fn register_materials(&self, _materials: &mut MaterialRegistry) {}

    fn create_generator(&self, registry: &BlockRegistry) -> Arc<dyn WorldGenerator>;
    fn spawn_position(&self) -> Vec3;
    fn default_place_block(&self, registry: &BlockRegistry) -> BlockId;

    /// Simulation and physics configuration for this game mode. Defaults to standard survival physics.
    fn simulation_rules(&self) -> SimulationRules {
        SimulationRules::default()
    }

    /// Pluggable environment and atmospheric provider. Defaults to `StandardEnvironment`.
    fn create_environment(&self) -> Arc<dyn EnvironmentProvider> {
        Arc::new(StandardEnvironment)
    }

    /// Pluggable avatar model configuration. Defaults to `AvatarModel::default()`.
    fn create_avatar(&self) -> AvatarModel {
        AvatarModel::default()
    }
}

/// Configurable avatar visual style provided by a game mode.
#[derive(Debug, Clone)]
pub enum AvatarModel {
    /// Pure first-person camera with no visible character body.
    None,
    /// Stylized low-poly humanoid survivor (Unturned / classic blocky survivor style).
    SimpleHumanoid {
        skin_color: [f32; 3],
        shirt_color: [f32; 3],
        pants_color: [f32; 3],
        hair_color: [f32; 3],
    },
}

impl Default for AvatarModel {
    fn default() -> Self {
        Self::SimpleHumanoid {
            skin_color: [0.92, 0.76, 0.62],
            shirt_color: [0.22, 0.45, 0.85],
            pants_color: [0.18, 0.22, 0.32],
            hair_color: [0.25, 0.18, 0.12],
        }
    }
}

/// Dynamic environment state computed for a frame.
#[derive(Debug, Clone, Copy)]
pub struct EnvironmentState {
    /// Normalized direction pointing towards the primary light source (sun or moon).
    pub sun_dir: Vec3,
    /// Direct sunlight RGB color.
    pub sun_color: [f32; 3],
    /// Sunlight intensity multiplier.
    pub sun_intensity: f32,
    /// Ambient light coming from the sky dome.
    pub ambient_sky: [f32; 3],
    /// Ambient light bouncing from the ground.
    pub ambient_ground: [f32; 3],
    /// Fog / horizon clearing color.
    pub fog_color: [f32; 3],
    /// Distance multiplier for fog start/end.
    pub fog_density: f32,
    /// Sky zenith color for procedural sky rendering.
    pub sky_zenith: [f32; 3],
    /// Sky horizon color for procedural sky rendering.
    pub sky_horizon: [f32; 3],
}

impl Default for EnvironmentState {
    fn default() -> Self {
        Self {
            sun_dir: Vec3::new(0.55, 0.82, 0.38).normalize(),
            sun_color: [1.0, 0.95, 0.85],
            sun_intensity: 1.0,
            ambient_sky: [0.46, 0.71, 0.96],
            ambient_ground: [0.25, 0.22, 0.18],
            fog_color: [0.46, 0.71, 0.96],
            fog_density: 1.0,
            sky_zenith: [0.25, 0.50, 0.85],
            sky_horizon: [0.70, 0.82, 0.95],
        }
    }
}

/// Pluggable environment and atmospheric model.
pub trait EnvironmentProvider: Send + Sync {
    /// Evaluates environmental lighting and sky colors for the given time of day (0.0 .. 24.0 hours).
    fn evaluate(&self, time_of_day: f32) -> EnvironmentState;
}

/// Standard procedural earth-like day/night atmospheric cycle.
#[derive(Debug, Clone, Default)]
pub struct StandardEnvironment;

impl EnvironmentProvider for StandardEnvironment {
    fn evaluate(&self, time_of_day: f32) -> EnvironmentState {
        let hour = time_of_day.rem_euclid(24.0);
        let angle = (hour - 6.0) / 24.0 * std::f32::consts::TAU;
        let sun_elevation = angle.sin();
        let sun_azimuth = angle.cos();

        // Actual continuous sun direction across the entire 24h orbit
        let actual_sun_dir = Vec3::new(sun_azimuth * 0.70, sun_elevation, 0.45).normalize();

        let is_day = sun_elevation >= -0.06;
        let primary_light_dir = if is_day {
            Vec3::new(actual_sun_dir.x, actual_sun_dir.y.max(0.04), actual_sun_dir.z).normalize()
        } else {
            Vec3::new(-actual_sun_dir.x, (-actual_sun_dir.y).max(0.04), -actual_sun_dir.z).normalize()
        };

        let day_factor = (sun_elevation * 3.5).clamp(0.0, 1.0);
        let sunset_factor =
            (1.0 - (sun_elevation.abs() * 5.0).min(1.0)) * (1.0 - day_factor * 0.35);

        let day_zenith = [0.16, 0.38, 0.82];
        let day_horizon = [0.65, 0.78, 0.92];
        let night_zenith = [0.015, 0.025, 0.055];
        let night_horizon = [0.035, 0.055, 0.10];
        let sunset_color = [0.96, 0.46, 0.18];

        let lerp3 = |a: [f32; 3], b: [f32; 3], t: f32| -> [f32; 3] {
            [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ]
        };

        let mut zenith = lerp3(night_zenith, day_zenith, day_factor);
        let mut horizon = lerp3(night_horizon, day_horizon, day_factor);
        horizon = lerp3(horizon, sunset_color, sunset_factor * 0.85);
        zenith = lerp3(zenith, [0.42, 0.22, 0.36], sunset_factor * 0.45);

        let (sun_color, sun_intensity) = if is_day {
            let col = lerp3(sunset_color, [1.0, 0.95, 0.86], day_factor);
            (col, 0.12 + 0.78 * day_factor)
        } else {
            ([0.45, 0.58, 0.85], 0.12)
        };

        EnvironmentState {
            sun_dir: primary_light_dir,
            sun_color,
            sun_intensity,
            ambient_sky: lerp3([0.02, 0.03, 0.06], [0.22, 0.30, 0.42], day_factor),
            ambient_ground: lerp3([0.01, 0.01, 0.02], [0.10, 0.09, 0.07], day_factor),
            fog_color: horizon,
            fog_density: 1.0,
            sky_zenith: zenith,
            sky_horizon: horizon,
        }
    }
}

/// A dynamic point light source (e.g. hand lantern, torch, glow block, projectile).
#[derive(Debug, Clone, Copy)]
pub struct PointLight {
    pub position: Vec3,
    pub color: [f32; 3],
    pub radius: f32,
    pub intensity: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_transform_normalization_preserves_position() {
        let mut transform = RelativeTransform::new(
            ReferenceFrameId::UNIVERSE_ROOT,
            IVec3::new(10, -5, 2),
            Vec3::new(300.0, -50.0, 512.0),
        );
        transform.normalize();

        assert!(transform.local_offset.x >= 0.0 && transform.local_offset.x < SECTOR_SIZE);
        assert!(transform.local_offset.y >= 0.0 && transform.local_offset.y < SECTOR_SIZE);
        assert!(transform.local_offset.z >= 0.0 && transform.local_offset.z < SECTOR_SIZE);

        let original_total = Vec3::new(
            10.0 * SECTOR_SIZE + 300.0,
            -5.0 * SECTOR_SIZE - 50.0,
            2.0 * SECTOR_SIZE + 512.0,
        );
        let normalized_total = Vec3::new(
            transform.sector.x as f32 * SECTOR_SIZE + transform.local_offset.x,
            transform.sector.y as f32 * SECTOR_SIZE + transform.local_offset.y,
            transform.sector.z as f32 * SECTOR_SIZE + transform.local_offset.z,
        );

        assert!((original_total - normalized_total).length() < 0.001);
    }

    #[test]
    fn relative_transform_reparenting_seamless() {
        let ship_frame = ReferenceFrameId(100);
        let station_frame = ReferenceFrameId(200);

        // Ship is located at (100, 0, 0) relative to station, moving at 20 m/s in X
        let ship_in_station = RelativeTransform {
            parent_frame: station_frame,
            sector: IVec3::ZERO,
            local_offset: Vec3::new(100.0, 0.0, 0.0),
            local_rotation: Quat::IDENTITY,
            local_velocity: Vec3::new(20.0, 0.0, 0.0),
        };

        // Player is walking inside the ship at (5, 0, 0) with relative speed 2 m/s
        let mut player = RelativeTransform {
            parent_frame: ship_frame,
            sector: IVec3::ZERO,
            local_offset: Vec3::new(5.0, 0.0, 0.0),
            local_rotation: Quat::IDENTITY,
            local_velocity: Vec3::new(2.0, 0.0, 0.0),
        };

        player.reparent_to(station_frame, &ship_in_station, Vec3::ZERO);

        assert_eq!(player.parent_frame, station_frame);
        assert!((player.local_offset.x - 105.0).abs() < 0.001);
        assert!((player.local_velocity.x - 22.0).abs() < 0.001);
    }

    #[test]
    fn grid_container_item_packing_and_rotation() {
        let mut container = GridContainer::new(10, 10, 50.0);

        let rifle = ItemStack::new(BlockId(1), 1);
        let idx = container.auto_pack(rifle, (4, 2), 3.5).expect("should fit rifle");
        assert_eq!(container.items[idx].effective_size(), (4, 2));

        // Rotate rifle
        assert!(container.rotate_item(idx).is_ok());
        assert_eq!(container.items[idx].effective_size(), (2, 4));

        // Insert second item next to it
        let medkit = ItemStack::new(BlockId(2), 1);
        assert!(container.auto_pack(medkit, (2, 2), 0.5).is_ok());

        assert!((container.total_weight() - 4.0).abs() < 0.001);
    }
}
