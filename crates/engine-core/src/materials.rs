use std::collections::HashMap;

/// Unique identifier for a registered material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MaterialId(pub u16);

impl MaterialId {
    pub const DEFAULT: Self = Self(0);
}

/// Shading model determining how the material interacts with light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum ShadingModel {
    /// Physically-Based Rendering with Cook-Torrance GGX specular, roughness, metallic (7 Days to Die / Realistic).
    #[default]
    StandardPbr = 0,
    /// Stylized banded cel/toon shading with clean saturated tones and no micro-noise (Unturned style).
    Stylized = 1,
    /// Organic foliage with subsurface transmission (sun shines through leaves/grass) and wind sway.
    Foliage = 2,
    /// Self-illuminated emissive material independent of sun shadows (lamps, torches, glowing ore).
    Emissive = 3,
    /// Realistic animated water with waves, Fresnel reflection, caustics, and foam.
    Water = 4,
}

/// Transparency / blending mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum AlphaMode {
    #[default]
    Opaque = 0,
    Cutout = 1,
    Blend = 2,
}

/// Data-driven definition of a material that can be applied to voxels or 3D models.
/// Extended with physical, ballistic, and acoustic properties for shooters, survival, and physics simulations.
#[derive(Debug, Clone)]
pub struct MaterialDef {
    pub name: String,
    pub shading_model: ShadingModel,
    pub alpha_mode: AlphaMode,
    /// Tint color multiplied with texture (or standalone color if untextured).
    pub base_color: [f32; 4],
    /// Texture array index (0 = untextured/solid color, 1..=N = texture atlas layer).
    pub texture_id: u16,
    /// Surface roughness from 0.0 (mirror-smooth) to 1.0 (chalk-rough).
    pub roughness: f32,
    /// Metallic factor from 0.0 (dielectric) to 1.0 (conductor).
    pub metallic: f32,
    /// Self-illumination color and intensity.
    pub emission: [f32; 3],
    /// Foliage Subsurface Transmission strength (0.0 to 1.0).
    pub subsurface: f32,

    // --- Physical, Ballistic, and Structural Parameters ---
    /// Material density (in g/cm^3 or relative penetration resistance multiplier).
    /// Used for bullet penetration calculations in shooters.
    pub density: f32,
    /// Probability (0.0 to 1.0) of projectile ricochet at shallow angles.
    pub ricochet_chance: f32,
    /// Sound transmission loss coefficient (0.0 = acoustically transparent, 1.0 = full sound blockage).
    pub sound_absorption: f32,
    /// Maximum structural weight/load capacity in units before collapse under gravity/stress.
    pub max_structural_load: u16,
}

impl MaterialDef {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            shading_model: ShadingModel::StandardPbr,
            alpha_mode: AlphaMode::Opaque,
            base_color: [1.0, 1.0, 1.0, 1.0],
            texture_id: 0,
            roughness: 0.85,
            metallic: 0.0,
            emission: [0.0, 0.0, 0.0],
            subsurface: 0.0,
            density: 1.0,
            ricochet_chance: 0.05,
            sound_absorption: 0.25,
            max_structural_load: 100,
        }
    }

    pub fn stylized(name: impl Into<String>, color: [f32; 3]) -> Self {
        Self::new(name)
            .with_shading(ShadingModel::Stylized)
            .with_color(color)
            .with_roughness(0.9)
            .with_density(1.0)
            .with_ricochet(0.05)
            .with_sound_absorption(0.2)
            .with_structural_load(80)
    }

    pub fn foliage(name: impl Into<String>, texture_id: u16, tint: [f32; 3]) -> Self {
        Self::new(name)
            .with_shading(ShadingModel::Foliage)
            .with_alpha(AlphaMode::Cutout)
            .with_texture(texture_id)
            .with_color(tint)
            .with_roughness(0.85)
            .with_subsurface(0.65)
            .with_density(0.15)
            .with_ricochet(0.0)
            .with_sound_absorption(0.1)
            .with_structural_load(15)
    }

    pub fn pbr(name: impl Into<String>, texture_id: u16, roughness: f32, metallic: f32) -> Self {
        let is_metal = metallic > 0.5;
        let density = if is_metal { 7.8 } else { 2.4 };
        let ricochet = if is_metal { 0.45 } else { 0.12 };
        let absorption = if is_metal { 0.65 } else { 0.55 };
        let load = if is_metal { 1200 } else { 450 };

        Self::new(name)
            .with_shading(ShadingModel::StandardPbr)
            .with_texture(texture_id)
            .with_roughness(roughness)
            .with_metallic(metallic)
            .with_density(density)
            .with_ricochet(ricochet)
            .with_sound_absorption(absorption)
            .with_structural_load(load)
    }

    pub fn emissive(name: impl Into<String>, color: [f32; 3], strength: f32) -> Self {
        Self::new(name)
            .with_shading(ShadingModel::Emissive)
            .with_color(color)
            .with_emission([color[0] * strength, color[1] * strength, color[2] * strength])
            .with_roughness(0.3)
            .with_density(0.8)
            .with_ricochet(0.05)
            .with_sound_absorption(0.2)
            .with_structural_load(50)
    }

    pub fn with_shading(mut self, model: ShadingModel) -> Self {
        self.shading_model = model;
        self
    }

    pub fn with_alpha(mut self, alpha: AlphaMode) -> Self {
        self.alpha_mode = alpha;
        self
    }

    pub fn with_color(mut self, color: [f32; 3]) -> Self {
        self.base_color = [color[0], color[1], color[2], 1.0];
        self
    }

    pub fn with_rgba(mut self, color: [f32; 4]) -> Self {
        self.base_color = color;
        self
    }

    pub fn with_texture(mut self, texture_id: u16) -> Self {
        self.texture_id = texture_id;
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

    pub fn with_emission(mut self, emission: [f32; 3]) -> Self {
        self.emission = emission;
        self
    }

    pub fn with_subsurface(mut self, sss: f32) -> Self {
        self.subsurface = sss.clamp(0.0, 1.0);
        self
    }

    pub fn with_density(mut self, density: f32) -> Self {
        self.density = density.max(0.0);
        self
    }

    pub fn with_ricochet(mut self, chance: f32) -> Self {
        self.ricochet_chance = chance.clamp(0.0, 1.0);
        self
    }

    pub fn with_sound_absorption(mut self, absorption: f32) -> Self {
        self.sound_absorption = absorption.clamp(0.0, 1.0);
        self
    }

    pub fn with_structural_load(mut self, load: u16) -> Self {
        self.max_structural_load = load;
        self
    }

    pub fn with_ballistics(
        mut self,
        density: f32,
        ricochet: f32,
        sound_absorption: f32,
        max_structural_load: u16,
    ) -> Self {
        self.density = density.max(0.0);
        self.ricochet_chance = ricochet.clamp(0.0, 1.0);
        self.sound_absorption = sound_absorption.clamp(0.0, 1.0);
        self.max_structural_load = max_structural_load;
        self
    }
}

/// Central registry managing all materials for voxels and 3D models.
#[derive(Debug, Clone)]
pub struct MaterialRegistry {
    defs: Vec<MaterialDef>,
    by_name: HashMap<String, MaterialId>,
}

impl Default for MaterialRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MaterialRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            defs: Vec::new(),
            by_name: HashMap::new(),
        };
        reg.register(MaterialDef::new("core:default"));
        reg
    }

    /// Creates a material registry pre-populated with standard physical materials.
    pub fn with_builtins() -> Self {
        let mut reg = Self::new();

        // 1. Metals
        reg.register(
            MaterialDef::pbr("builtin:gold", 19, 0.20, 0.95)
                .with_color([1.0, 0.85, 0.3])
                .with_ballistics(19.3, 0.30, 0.75, 600),
        );
        reg.register(
            MaterialDef::pbr("builtin:iron", 20, 0.28, 0.88)
                .with_color([0.92, 0.92, 0.95])
                .with_ballistics(7.87, 0.50, 0.70, 1500),
        );
        reg.register(
            MaterialDef::pbr("builtin:steel", 0, 0.22, 0.90)
                .with_color([0.80, 0.82, 0.85])
                .with_ballistics(8.05, 0.60, 0.75, 2200),
        );
        reg.register(
            MaterialDef::pbr("builtin:copper", 0, 0.25, 0.92)
                .with_color([0.95, 0.64, 0.54])
                .with_ballistics(8.96, 0.40, 0.65, 1100),
        );
        reg.register(
            MaterialDef::pbr("builtin:chrome", 0, 0.08, 0.98)
                .with_color([0.96, 0.96, 0.98])
                .with_ballistics(7.19, 0.65, 0.80, 1800),
        );

        // 2. Minerals & Terrain
        reg.register(
            MaterialDef::pbr("builtin:stone", 4, 0.80, 0.0)
                .with_ballistics(2.65, 0.20, 0.60, 500),
        );
        reg.register(
            MaterialDef::pbr("builtin:cobblestone", 5, 0.85, 0.0)
                .with_ballistics(2.50, 0.15, 0.55, 400),
        );
        reg.register(
            MaterialDef::pbr("builtin:dirt", 3, 0.95, 0.0)
                .with_ballistics(1.30, 0.01, 0.85, 60),
        );
        reg.register(
            MaterialDef::pbr("builtin:sand", 6, 0.95, 0.0)
                .with_ballistics(1.60, 0.01, 0.90, 40),
        );
        reg.register(
            MaterialDef::pbr("builtin:bricks", 12, 0.85, 0.0)
                .with_ballistics(2.10, 0.18, 0.65, 650),
        );
        reg.register(
            MaterialDef::pbr("builtin:andesite", 23, 0.82, 0.0)
                .with_ballistics(2.60, 0.22, 0.62, 550),
        );
        reg.register(
            MaterialDef::pbr("builtin:diamond_block", 21, 0.15, 0.35)
                .with_ballistics(3.51, 0.70, 0.85, 5000),
        );
        reg.register(
            MaterialDef::pbr("builtin:diamond_ore", 22, 0.75, 0.0)
                .with_ballistics(2.80, 0.25, 0.65, 750),
        );

        // 3. Foliage & Organics (with SSS)
        reg.register(
            MaterialDef::foliage("builtin:grass_top", 1, [0.34, 0.76, 0.22])
                .with_subsurface(0.35)
                .with_roughness(0.92)
                .with_ballistics(0.20, 0.0, 0.15, 10),
        );
        reg.register(
            MaterialDef::foliage("builtin:oak_leaves", 10, [0.26, 0.66, 0.18])
                .with_subsurface(0.70)
                .with_roughness(0.88)
                .with_ballistics(0.15, 0.0, 0.10, 10),
        );
        reg.register(
            MaterialDef::pbr("builtin:oak_log", 8, 0.85, 0.0)
                .with_ballistics(0.75, 0.05, 0.45, 300),
        );
        reg.register(
            MaterialDef::pbr("builtin:oak_planks", 7, 0.75, 0.0)
                .with_ballistics(0.65, 0.06, 0.40, 250),
        );

        // 4. Stylized / Unturned materials
        reg.register(
            MaterialDef::stylized("builtin:unturned_skin", [0.94, 0.76, 0.62])
                .with_ballistics(1.0, 0.02, 0.20, 50),
        );
        reg.register(
            MaterialDef::stylized("builtin:unturned_hair", [0.35, 0.22, 0.12])
                .with_ballistics(0.3, 0.0, 0.10, 20),
        );
        reg.register(
            MaterialDef::stylized("builtin:unturned_shirt", [0.22, 0.55, 0.78])
                .with_ballistics(0.4, 0.01, 0.15, 30),
        );
        reg.register(
            MaterialDef::stylized("builtin:unturned_pants", [0.18, 0.25, 0.38])
                .with_ballistics(0.5, 0.02, 0.18, 40),
        );

        // 5. Special / Emissive
        reg.register(
            MaterialDef::emissive("builtin:warm_lamp", [1.0, 0.85, 0.45], 1.2)
                .with_ballistics(0.5, 0.02, 0.15, 40),
        );
        reg.register(
            MaterialDef::emissive("builtin:neon_blue", [0.2, 0.8, 1.0], 1.5)
                .with_ballistics(0.5, 0.02, 0.15, 40),
        );
        reg.register(
            MaterialDef::new("builtin:glass")
                .with_alpha(AlphaMode::Cutout)
                .with_texture(11)
                .with_roughness(0.05)
                .with_metallic(0.1)
                .with_ballistics(0.40, 0.02, 0.15, 20),
        );

        reg
    }

    pub fn register(&mut self, def: MaterialDef) -> MaterialId {
        if let Some(&existing) = self.by_name.get(&def.name) {
            return existing;
        }
        let id = MaterialId(
            u16::try_from(self.defs.len()).expect("material registry exceeded u16::MAX entries"),
        );
        self.by_name.insert(def.name.clone(), id);
        self.defs.push(def);
        id
    }

    pub fn get(&self, id: MaterialId) -> Option<&MaterialDef> {
        self.defs.get(id.0 as usize)
    }

    pub fn find(&self, name: &str) -> Option<MaterialId> {
        self.by_name.get(name).copied()
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &MaterialDef> {
        self.defs.iter()
    }
}
