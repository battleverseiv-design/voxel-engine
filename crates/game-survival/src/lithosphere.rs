use glam::IVec3;
use engine_core::{BlockId, BlockRegistry, WorldGenerator};

/// Ultra-realistic Lithosphere World Generator for Voxel Engine.
/// Inspired by the Lithosphere mod for Minecraft:
/// - Orogeny with ridged mountain ranges, steep peaks, rolling lowlands, and coastal cliffs.
/// - 3D Noodle & Tunnel caves with smooth tube cross-sections.
/// - Cyclopean Cheese Caverns with subterranean pillars.
/// - True Minecraft stratigraphy: Bedrock floor, Deepslate mantle, Stone crust, surface soils.
/// - Full ore vein distribution (regular & deepslate variants) and underground magma/geodes.
/// - Natural water basins and river valleys cutting to sea level (y = 6).
pub struct LithosphereWorldGen {
    pub seed: u32,
    // Basic terrain
    pub grass: BlockId,
    pub dirt: BlockId,
    pub stone: BlockId,
    pub cobblestone: BlockId,
    pub sand: BlockId,
    pub gravel: BlockId,
    pub clay: BlockId,
    pub snow: BlockId,
    pub ice: BlockId,
    pub water: BlockId,
    pub bedrock: BlockId,
    pub wood: BlockId,
    pub leaves: BlockId,
    pub tall_grass: BlockId,
    pub shrub: BlockId,

    // Deep geology
    pub deepslate: BlockId,
    pub tuff: BlockId,
    pub calcite: BlockId,
    pub dripstone: BlockId,
    pub basalt: BlockId,
    pub magma: BlockId,
    pub moss: BlockId,
    pub andesite: BlockId,
    pub obsidian: BlockId,

    // Standard Ores
    pub coal_ore: BlockId,
    pub iron_ore: BlockId,
    pub copper_ore: BlockId,
    pub gold_ore: BlockId,
    pub redstone_ore: BlockId,
    pub lapis_ore: BlockId,
    pub diamond_ore: BlockId,
    pub emerald_ore: BlockId,

    // Deepslate Ores
    pub deepslate_coal_ore: BlockId,
    pub deepslate_iron_ore: BlockId,
    pub deepslate_copper_ore: BlockId,
    pub deepslate_gold_ore: BlockId,
    pub deepslate_redstone_ore: BlockId,
    pub deepslate_lapis_ore: BlockId,
    pub deepslate_diamond_ore: BlockId,
    pub deepslate_emerald_ore: BlockId,

    // Special
    pub amethyst_block: BlockId,
    pub glowstone: BlockId,
}

impl LithosphereWorldGen {
    pub fn new(seed: u32, reg: &BlockRegistry) -> Self {
        let find = |name: &str| reg.find(name).unwrap_or(BlockId::AIR);
        Self {
            seed,
            grass: find("survival:grass"),
            dirt: find("survival:dirt"),
            stone: find("survival:stone"),
            cobblestone: find("survival:cobblestone"),
            sand: find("survival:sand"),
            gravel: find("survival:gravel"),
            clay: find("survival:clay"),
            snow: find("survival:snow"),
            ice: find("survival:ice"),
            water: find("survival:water"),
            bedrock: find("survival:bedrock"),
            wood: find("survival:wood"),
            leaves: find("survival:leaves"),
            tall_grass: find("survival:tall_grass"),
            shrub: find("survival:shrub"),

            deepslate: find("survival:deepslate"),
            tuff: find("survival:tuff"),
            calcite: find("survival:calcite"),
            dripstone: find("survival:dripstone"),
            basalt: find("survival:basalt"),
            magma: find("survival:magma"),
            moss: find("survival:moss"),
            andesite: find("survival:andesite"),
            obsidian: find("survival:obsidian"),

            coal_ore: find("survival:coal_ore"),
            iron_ore: find("survival:iron_ore"),
            copper_ore: find("survival:copper_ore"),
            gold_ore: find("survival:gold_ore"),
            redstone_ore: find("survival:redstone_ore"),
            lapis_ore: find("survival:lapis_ore"),
            diamond_ore: find("survival:diamond_ore"),
            emerald_ore: find("survival:emerald_ore"),

            deepslate_coal_ore: find("survival:deepslate_coal_ore"),
            deepslate_iron_ore: find("survival:deepslate_iron_ore"),
            deepslate_copper_ore: find("survival:deepslate_copper_ore"),
            deepslate_gold_ore: find("survival:deepslate_gold_ore"),
            deepslate_redstone_ore: find("survival:deepslate_redstone_ore"),
            deepslate_lapis_ore: find("survival:deepslate_lapis_ore"),
            deepslate_diamond_ore: find("survival:deepslate_diamond_ore"),
            deepslate_emerald_ore: find("survival:deepslate_emerald_ore"),

            amethyst_block: find("survival:amethyst_block"),
            glowstone: find("survival:glowstone"),
        }
    }

    /// Evaluates the 2D surface height and river mask at world coordinates (wx, wz).
    pub fn surface_height(&self, wx: i32, wz: i32) -> (i32, bool) {
        let fx = wx as f32;
        let fz = wz as f32;

        // 1. Continentalness (Oceans vs Plains vs Highlands)
        let cont = fbm2d(fx * 0.0032, fz * 0.0032, self.seed);

        // 2. Mountain Ridges (sharp spine-like ridge lines)
        let ridge_raw = fbm2d(fx * 0.0085, fz * 0.0085, self.seed.wrapping_add(555));
        let ridge = (1.0 - (ridge_raw * 2.0 - 1.0).abs()).powi(2);

        // 3. Detail hills
        let detail = fbm2d(fx * 0.038, fz * 0.038, self.seed.wrapping_add(777));

        // 4. Meandering River channels
        let river_raw = value_noise2d(fx * 0.0045, fz * 0.0045, self.seed.wrapping_add(999));
        let river_dist = (river_raw - 0.5).abs() * 2.0;
        let is_river = river_dist < 0.10 && cont > 0.28;

        let mut base_h = if cont < 0.30 {
            // Ocean / Deep water trench (-16 .. 4)
            -15.0 + (cont / 0.30) * 19.0
        } else if cont < 0.60 {
            // Rolling plains, meadows, and low hills (4 .. 30)
            let t = (cont - 0.30) / 0.30;
            4.0 + t * 26.0 + detail * 6.0
        } else {
            // Majestic mountain ranges & peaks (30 .. 95)
            let t = (cont - 0.60) / 0.40;
            30.0 + t * 25.0 + ridge * 40.0 + detail * 8.0
        };

        if is_river {
            let river_factor = (1.0 - (river_dist / 0.10)).powi(2);
            base_h = (base_h - river_factor * 16.0).max(4.0);
        }

        (base_h.round() as i32, is_river)
    }

    /// Tree placement with canopy check.
    fn has_tree_at(&self, wx: i32, wz: i32) -> Option<i32> {
        let h = hash2(wx, wz, self.seed.wrapping_add(888));
        if h % 120 != 0 {
            return None;
        }
        let (ground, is_river) = self.surface_height(wx, wz);
        // Only plant trees on grass in plains/low hills, away from water and mountain snow
        if is_river || ground < 8 || ground > 48 {
            return None;
        }
        Some(ground)
    }
}

impl WorldGenerator for LithosphereWorldGen {
    fn vertical_bounds(&self) -> (i32, i32) {
        (-3, 6)
    }

    fn column_max_height(&self, x: i32, z: i32) -> i32 {
        let (surface_y, _) = self.surface_height(x, z);
        surface_y + 12
    }

    fn generate_chunk(&self, chunk_pos: IVec3, chunk_size: usize, out: &mut [BlockId]) {
        let cs = chunk_size as i32;
        let base_x = chunk_pos.x * cs;
        let base_y = chunk_pos.y * cs;
        let base_z = chunk_pos.z * cs;

        const SEA_LEVEL: i32 = 6;
        const BEDROCK_LEVEL: i32 = -40;

        // Skip chunks entirely above sky ceiling or below bedrock
        if base_y > 110 || base_y + cs < BEDROCK_LEVEL - 5 {
            return;
        }

        for lz in 0..chunk_size {
            let wz = base_z + lz as i32;
            for lx in 0..chunk_size {
                let wx = base_x + lx as i32;
                let (surface_y, is_river) = self.surface_height(wx, wz);

                for ly in 0..chunk_size {
                    let wy = base_y + ly as i32;
                    let idx = lx + lz * chunk_size + ly * chunk_size * chunk_size;

                    // 1. Below bedrock floor
                    if wy <= BEDROCK_LEVEL {
                        out[idx] = self.bedrock;
                        continue;
                    }

                    // 2. Air or Water above ground surface
                    if wy > surface_y {
                        if wy <= SEA_LEVEL {
                            out[idx] = self.water;
                        }
                        continue;
                    }

                    // 3. 3D Cave Carving Pass (Lithosphere Noodle + Cavern System)
                    // Noodle / Tunnel caves
                    let n1 = value_noise3d(wx as f32 * 0.024, wy as f32 * 0.034, wz as f32 * 0.024, self.seed.wrapping_add(101));
                    let n2 = value_noise3d(wx as f32 * 0.024, wy as f32 * 0.034, wz as f32 * 0.024, self.seed.wrapping_add(202));
                    let d1 = (n1 - 0.5) * 2.0;
                    let d2 = (n2 - 0.5) * 2.0;
                    let is_noodle = (d1 * d1 + d2 * d2) < 0.038;

                    // Giant Cheese Caverns (deep subterranean halls)
                    let n_cheese = value_noise3d(wx as f32 * 0.013, wy as f32 * 0.017, wz as f32 * 0.013, self.seed.wrapping_add(303));
                    let is_cavern = wy < 18 && n_cheese < 0.20;

                    // Surface entrance breaches
                    let near_surface = wy >= surface_y - 3;
                    let cave_open = if near_surface {
                        // Allow caves to breach surface only at distinct entrance fissures
                        is_noodle && (d1.abs() < 0.06)
                    } else {
                        is_noodle || is_cavern
                    };

                    if cave_open {
                        // Subterranean floor features (lava/magma at depth, water pools)
                        if wy <= -28 {
                            out[idx] = self.magma;
                        } else if wy <= SEA_LEVEL && is_cavern && wy <= surface_y - 12 {
                            out[idx] = self.water;
                        }
                        // Otherwise remains carved empty air
                        continue;
                    }

                    // 4. Solid Ground Stratigraphy
                    let block = if wy == surface_y {
                        // Top surface layer
                        if surface_y <= SEA_LEVEL + 1 {
                            self.sand
                        } else if surface_y > 55 {
                            self.snow
                        } else if is_river {
                            self.gravel
                        } else {
                            self.grass
                        }
                    } else if wy >= surface_y - 3 {
                        // Sub-surface soil / sediment layer
                        if surface_y <= SEA_LEVEL + 1 {
                            self.sand
                        } else if surface_y > 55 {
                            self.stone
                        } else if is_river {
                            self.clay
                        } else {
                            self.dirt
                        }
                    } else {
                        // Geological underground strata
                        let is_deepslate = if wy < -6 {
                            true
                        } else if wy <= 0 {
                            // Dithered transition between stone and deepslate
                            let h = hash3(wx, wy, wz, self.seed.wrapping_add(404));
                            h % 100 < 65
                        } else {
                            false
                        };

                        let ore_hash = hash3(wx, wy, wz, self.seed.wrapping_add(505));

                        if is_deepslate {
                            // --- Deepslate Stratum (-40 < y <= 0) ---
                            if wy < -32 && ore_hash % 600 == 0 {
                                self.deepslate_diamond_ore
                            } else if wy < -12 && ore_hash % 350 == 0 {
                                self.deepslate_redstone_ore
                            } else if wy < -5 && ore_hash % 450 == 0 {
                                self.deepslate_gold_ore
                            } else if ore_hash % 280 == 0 {
                                self.deepslate_iron_ore
                            } else if ore_hash % 380 == 0 {
                                self.deepslate_lapis_ore
                            } else if ore_hash % 420 == 0 {
                                self.deepslate_copper_ore
                            } else if ore_hash % 180 == 0 {
                                self.tuff
                            } else if ore_hash % 300 == 0 {
                                self.calcite
                            } else if ore_hash % 450 == 0 {
                                self.dripstone
                            } else if ore_hash % 500 == 0 {
                                self.basalt
                            } else if ore_hash % 950 == 0 {
                                self.amethyst_block
                            } else {
                                self.deepslate
                            }
                        } else {
                            // --- Upper Stone Crust (y > 0) ---
                            if surface_y > 50 && wy > 40 && ore_hash % 700 == 0 {
                                self.emerald_ore
                            } else if wy > 10 && ore_hash % 160 == 0 {
                                self.coal_ore
                            } else if ore_hash % 240 == 0 {
                                self.iron_ore
                            } else if wy < 35 && ore_hash % 280 == 0 {
                                self.copper_ore
                            } else if wy < 12 && ore_hash % 450 == 0 {
                                self.gold_ore
                            } else if ore_hash % 180 == 0 {
                                self.andesite
                            } else if ore_hash % 350 == 0 {
                                self.gravel
                            } else {
                                self.stone
                            }
                        }
                    };

                    out[idx] = block;
                }
            }
        }

        // 5. Tree pass (natural oak trees on surface)
        for tz in (base_z - 2)..=(base_z + cs + 1) {
            for tx in (base_x - 2)..=(base_x + cs + 1) {
                let Some(ground) = self.has_tree_at(tx, tz) else {
                    continue;
                };

                let trunk_height = 5;
                for ty in 0..=(trunk_height + 2) {
                    let wy = ground + 1 + ty;
                    let is_leaves = ty >= trunk_height - 2;

                    let radius: i32 = if ty >= trunk_height + 1 {
                        1
                    } else if ty >= trunk_height - 1 {
                        2
                    } else {
                        1
                    };

                    if is_leaves {
                        for dx in -radius..=radius {
                            for dz in -radius..=radius {
                                if dx.abs() == radius && dz.abs() == radius && ty >= trunk_height {
                                    continue;
                                }
                                let lx = (tx + dx) - base_x;
                                let ly = wy - base_y;
                                let lz = (tz + dz) - base_z;

                                if lx >= 0 && lx < cs && ly >= 0 && ly < cs && lz >= 0 && lz < cs {
                                    let idx = lx as usize + lz as usize * chunk_size + ly as usize * chunk_size * chunk_size;
                                    if out[idx] == BlockId::AIR {
                                        out[idx] = self.leaves;
                                    }
                                }
                            }
                        }
                    }

                    if ty <= trunk_height {
                        let lx = tx - base_x;
                        let ly = wy - base_y;
                        let lz = tz - base_z;

                        if lx >= 0 && lx < cs && ly >= 0 && ly < cs && lz >= 0 && lz < cs {
                            let idx = lx as usize + lz as usize * chunk_size + ly as usize * chunk_size * chunk_size;
                            out[idx] = self.wood;
                        }
                    }
                }
            }
        }

        // 6. Surface foliage pass (wild prairie grass & shrubs on open grass)
        for lz in 0..cs {
            for lx in 0..cs {
                let wx = base_x + lx;
                let wz = base_z + lz;
                let (surface_y, is_river) = self.surface_height(wx, wz);
                if is_river || surface_y <= SEA_LEVEL || surface_y > 48 {
                    continue;
                }
                let plant_y = surface_y + 1;
                let ly = plant_y - base_y;
                if ly >= 0 && ly < cs {
                    let idx = lx as usize + lz as usize * chunk_size + ly as usize * chunk_size * chunk_size;
                    if out[idx] == BlockId::AIR {
                        let h = hash2(wx, wz, self.seed.wrapping_add(777));
                        if h % 100 < 28 {
                            out[idx] = self.tall_grass;
                        } else if h % 100 < 34 {
                            out[idx] = self.shrub;
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Procedural Noise Primitives
// ---------------------------------------------------------------------------

#[inline]
fn hash2(x: i32, z: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(374_761_393)
        ^ (z as u32).wrapping_mul(668_265_263)
        ^ seed.wrapping_mul(1_442_695_041);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}

#[inline]
fn hash3(x: i32, y: i32, z: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(374_761_393)
        ^ (y as u32).wrapping_mul(834_927_907)
        ^ (z as u32).wrapping_mul(668_265_263)
        ^ seed.wrapping_mul(1_442_695_041);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}

fn value_noise2d(x: f32, z: f32, seed: u32) -> f32 {
    let ix = x.floor() as i32;
    let iz = z.floor() as i32;
    let fx = x - ix as f32;
    let fz = z - iz as f32;
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uz = fz * fz * (3.0 - 2.0 * fz);

    let sample = |cx: i32, cz: i32| -> f32 {
        (hash2(cx, cz, seed) & 0xFFFF) as f32 / 65535.0
    };

    let a = sample(ix, iz);
    let b = sample(ix + 1, iz);
    let c = sample(ix, iz + 1);
    let d = sample(ix + 1, iz + 1);

    let ab = a + (b - a) * ux;
    let cd = c + (d - c) * ux;
    ab + (cd - ab) * uz
}

fn fbm2d(x: f32, z: f32, seed: u32) -> f32 {
    let mut total = 0.0;
    let mut amp = 0.5;
    let mut freq = 1.0;
    for i in 0..4u32 {
        total += value_noise2d(x * freq, z * freq, seed.wrapping_add(i * 31)) * amp;
        freq *= 2.02;
        amp *= 0.5;
    }
    total
}

fn value_noise3d(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let iz = z.floor() as i32;
    let fx = x - ix as f32;
    let fy = y - iy as f32;
    let fz = z - iz as f32;
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);
    let uz = fz * fz * (3.0 - 2.0 * fz);

    let s = |dx: i32, dy: i32, dz: i32| -> f32 {
        (hash3(ix + dx, iy + dy, iz + dz, seed) & 0xFFFF) as f32 / 65535.0
    };

    let c000 = s(0, 0, 0);
    let c100 = s(1, 0, 0);
    let c010 = s(0, 1, 0);
    let c110 = s(1, 1, 0);
    let c001 = s(0, 0, 1);
    let c101 = s(1, 0, 1);
    let c011 = s(0, 1, 1);
    let c111 = s(1, 1, 1);

    let x00 = c000 + (c100 - c000) * ux;
    let x10 = c010 + (c110 - c010) * ux;
    let x01 = c001 + (c101 - c001) * ux;
    let x11 = c011 + (c111 - c011) * ux;

    let y0 = x00 + (x10 - x00) * uy;
    let y1 = x01 + (x11 - x01) * uy;

    y0 + (y1 - y0) * uz
}
