use std::sync::Arc;

pub mod lithosphere;
pub use lithosphere::LithosphereWorldGen;

use engine_core::{
    BlockDef, BlockGeometry, BlockId, BlockRegistry, BlockTransparency, GameMode, ShadingModel, WorldGenerator,
};
use glam::{IVec3, Vec3};

pub struct SurvivalMode {
    pub seed: u32,
    pub generator_type: String,
}

impl Default for SurvivalMode {
    fn default() -> Self {
        Self {
            seed: 42,
            generator_type: "Lithosphere".to_string(),
        }
    }
}

impl GameMode for SurvivalMode {
    fn id(&self) -> &'static str {
        "survival"
    }

    fn register_blocks(&self, registry: &mut BlockRegistry) {
        let white = [1.0, 1.0, 1.0];

        // 1. Grass block
        registry.register(
            BlockDef::solid_tinted("survival:grass", [0.46, 0.78, 0.32], white, [0.46, 0.78, 0.32])
                .with_face_textures(1, 2, 3)
                .with_hardness(0.6)
                .with_roughness(0.92)
                .with_metallic(0.0)
                .with_icon_color([97, 171, 64]),
        );

        // 2. Dirt
        registry.register(
            BlockDef::solid_tinted("survival:dirt", white, white, white)
                .with_texture(3)
                .with_hardness(0.5)
                .with_roughness(0.95)
                .with_icon_color([135, 97, 67]),
        );

        // 3. Smooth Stone
        registry.register(
            BlockDef::solid_tinted("survival:stone", white, white, white)
                .with_texture(4)
                .with_hardness(2.0)
                .with_roughness(0.80)
                .with_icon_color([125, 125, 125]),
        );

        // 4. Cobblestone
        registry.register(
            BlockDef::solid_tinted("survival:cobblestone", white, white, white)
                .with_texture(5)
                .with_hardness(1.8)
                .with_roughness(0.85)
                .with_icon_color([129, 129, 129]),
        );

        // 5. Sand
        registry.register(
            BlockDef::solid_tinted("survival:sand", white, white, white)
                .with_texture(6)
                .with_hardness(0.4)
                .with_roughness(0.95)
                .with_icon_color([219, 206, 162]),
        );

        // 6. Oak Log / Wood
        registry.register(
            BlockDef::solid_tinted("survival:wood", white, white, white)
                .with_face_textures(9, 8, 9)
                .with_hardness(1.2)
                .with_roughness(0.85)
                .with_icon_color([107, 84, 49]),
        );

        // 7. Oak Planks
        registry.register(
            BlockDef::solid_tinted("survival:planks", white, white, white)
                .with_texture(7)
                .with_hardness(1.0)
                .with_roughness(0.75)
                .with_icon_color([171, 138, 84]),
        );

        // 8. Oak Leaves (cutout alpha, rich green foliage with organic SSS)
        registry.register(
            BlockDef::solid_tinted("survival:leaves", [0.38, 0.72, 0.22], [0.38, 0.72, 0.22], [0.38, 0.72, 0.22])
                .with_texture(10)
                .with_hardness(0.25)
                .with_transparency(BlockTransparency::Cutout)
                .with_shading(ShadingModel::Foliage)
                .with_subsurface(0.70)
                .with_roughness(0.88)
                .with_icon_color([86, 172, 52]),
        );

        // 9. Glass (cutout alpha)
        registry.register(
            BlockDef::solid_tinted("survival:glass", white, white, white)
                .with_texture(11)
                .with_hardness(0.3)
                .with_transparency(BlockTransparency::Cutout)
                .with_roughness(0.05)
                .with_metallic(0.1)
                .with_icon_color([177, 210, 221]),
        );

        // 10. Bricks
        registry.register(
            BlockDef::solid_tinted("survival:bricks", white, white, white)
                .with_texture(12)
                .with_hardness(2.0)
                .with_roughness(0.85)
                .with_icon_color([150, 91, 74]),
        );

        // 11. Bookshelf (wooden top/bottom, books on sides)
        registry.register(
            BlockDef::solid_tinted("survival:bookshelf", white, white, white)
                .with_face_textures(7, 13, 7)
                .with_roughness(0.80)
                .with_icon_color([104, 82, 50]),
        );

        // 12. Crafting Table (top planks, front with tools, patterned sides)
        registry.register(
            BlockDef::solid_tinted("survival:crafting_table", white, white, white)
                .with_custom_face_textures([7, 7, 14, 15, 15, 15])
                .with_roughness(0.75)
                .with_icon_color([147, 120, 78]),
        );

        // 13. Furnace (top stones, bottom stone, front furnace, sides stone)
        registry.register(
            BlockDef::solid_tinted("survival:furnace", white, white, white)
                .with_custom_face_textures([18, 4, 16, 17, 17, 17])
                .with_roughness(0.82)
                .with_icon_color([95, 95, 95]),
        );

        // 14. Gold Block (metallic)
        registry.register(
            BlockDef::solid_tinted("survival:gold", white, white, white)
                .with_texture(19)
                .with_metallic(0.95)
                .with_roughness(0.20)
                .with_icon_color([250, 214, 63]),
        );

        // 15. Iron Block (metallic)
        registry.register(
            BlockDef::solid_tinted("survival:iron", white, white, white)
                .with_texture(20)
                .with_metallic(0.88)
                .with_roughness(0.28)
                .with_icon_color([225, 225, 225]),
        );

        // 16. Diamond Block
        registry.register(
            BlockDef::solid_tinted("survival:diamond_block", white, white, white)
                .with_texture(21)
                .with_metallic(0.35)
                .with_roughness(0.15)
                .with_icon_color([105, 242, 231]),
        );

        // 17. Diamond Ore
        registry.register(
            BlockDef::solid_tinted("survival:diamond_ore", white, white, white)
                .with_texture(22)
                .with_roughness(0.75)
                .with_icon_color([122, 144, 143]),
        );

        // 18. Andesite
        registry.register(
            BlockDef::solid_tinted("survival:andesite", white, white, white)
                .with_texture(23)
                .with_roughness(0.82)
                .with_icon_color([137, 137, 138]),
        );

        // 19. Bedrock
        registry.register(
            BlockDef::solid_tinted("survival:bedrock", white, white, white)
                .with_texture(24)
                .with_hardness(9999.0)
                .with_roughness(0.90)
                .with_icon_color([123, 108, 88]),
        );

        // 20. Obsidian
        registry.register(
            BlockDef::solid_tinted("survival:obsidian", white, white, white)
                .with_texture(25)
                .with_hardness(15.0)
                .with_roughness(0.15)
                .with_icon_color([19, 17, 21]),
        );

        // 21. Stone Bricks
        registry.register(
            BlockDef::solid_tinted("survival:stone_bricks", white, white, white)
                .with_texture(26)
                .with_hardness(2.0)
                .with_roughness(0.80)
                .with_icon_color([120, 117, 111]),
        );

        // 22. Mossy Cobblestone
        registry.register(
            BlockDef::solid_tinted("survival:mossy_cobblestone", white, white, white)
                .with_texture(27)
                .with_hardness(2.0)
                .with_roughness(0.85)
                .with_icon_color([106, 99, 88]),
        );

        // 23. Smooth Stone
        registry.register(
            BlockDef::solid_tinted("survival:smooth_stone", white, white, white)
                .with_texture(28)
                .with_hardness(2.0)
                .with_roughness(0.80)
                .with_icon_color([105, 104, 102]),
        );

        // 24. Gravel
        registry.register(
            BlockDef::solid_tinted("survival:gravel", white, white, white)
                .with_texture(29)
                .with_hardness(0.6)
                .with_roughness(0.95)
                .with_icon_color([104, 102, 100]),
        );

        // 25. Clay
        registry.register(
            BlockDef::solid_tinted("survival:clay", white, white, white)
                .with_texture(30)
                .with_hardness(0.6)
                .with_roughness(0.90)
                .with_icon_color([175, 130, 54]),
        );

        // 26. Snow Block
        registry.register(
            BlockDef::solid_tinted("survival:snow", white, white, white)
                .with_texture(31)
                .with_hardness(0.3)
                .with_roughness(0.90)
                .with_icon_color([203, 208, 213]),
        );

        // 27. Ice Block
        registry.register(
            BlockDef::solid_tinted("survival:ice", white, white, white)
                .with_texture(32)
                .with_hardness(0.5)
                .with_transparency(BlockTransparency::Cutout)
                .with_roughness(0.05)
                .with_metallic(0.1)
                .with_icon_color([118, 137, 164]),
        );

        // 28. Coal Ore
        registry.register(
            BlockDef::solid_tinted("survival:coal_ore", white, white, white)
                .with_texture(33)
                .with_hardness(3.0)
                .with_roughness(0.80)
                .with_icon_color([66, 62, 58]),
        );

        // 29. Iron Ore
        registry.register(
            BlockDef::solid_tinted("survival:iron_ore", white, white, white)
                .with_texture(34)
                .with_hardness(3.0)
                .with_roughness(0.80)
                .with_icon_color([71, 65, 58]),
        );

        // 30. Gold Ore
        registry.register(
            BlockDef::solid_tinted("survival:gold_ore", white, white, white)
                .with_texture(35)
                .with_hardness(3.0)
                .with_roughness(0.80)
                .with_icon_color([75, 68, 56]),
        );

        // 31. Redstone Ore
        registry.register(
            BlockDef::solid_tinted("survival:redstone_ore", white, white, white)
                .with_texture(36)
                .with_hardness(3.0)
                .with_roughness(0.80)
                .with_icon_color([71, 58, 54]),
        );

        // 32. Redstone Block
        registry.register(
            BlockDef::solid_tinted("survival:redstone_block", white, white, white)
                .with_texture(37)
                .with_hardness(2.5)
                .with_roughness(0.40)
                .with_shading(ShadingModel::Emissive)
                .with_emission([0.45, 0.05, 0.05])
                .with_icon_color([127, 0, 0]),
        );

        // 33. Lapis Lazuli Ore
        registry.register(
            BlockDef::solid_tinted("survival:lapis_ore", white, white, white)
                .with_texture(38)
                .with_hardness(3.0)
                .with_roughness(0.80)
                .with_icon_color([62, 62, 67]),
        );

        // 34. Lapis Lazuli Block
        registry.register(
            BlockDef::solid_tinted("survival:lapis_block", white, white, white)
                .with_texture(39)
                .with_hardness(2.5)
                .with_roughness(0.35)
                .with_metallic(0.2)
                .with_icon_color([5, 34, 125]),
        );

        // 35. Emerald Ore
        registry.register(
            BlockDef::solid_tinted("survival:emerald_ore", white, white, white)
                .with_texture(40)
                .with_hardness(3.0)
                .with_roughness(0.80)
                .with_icon_color([60, 66, 57]),
        );

        // 36. Emerald Block
        registry.register(
            BlockDef::solid_tinted("survival:emerald_block", white, white, white)
                .with_texture(41)
                .with_hardness(2.5)
                .with_roughness(0.25)
                .with_metallic(0.3)
                .with_icon_color([16, 125, 5]),
        );

        // 37. Copper Ore
        registry.register(
            BlockDef::solid_tinted("survival:copper_ore", white, white, white)
                .with_texture(42)
                .with_hardness(3.0)
                .with_roughness(0.80)
                .with_icon_color([69, 69, 62]),
        );

        // 38. Copper Block
        registry.register(
            BlockDef::solid_tinted("survival:copper_block", white, white, white)
                .with_texture(43)
                .with_hardness(2.5)
                .with_roughness(0.30)
                .with_metallic(0.85)
                .with_icon_color([224, 129, 79]),
        );

        // 39. Amethyst Block
        registry.register(
            BlockDef::solid_tinted("survival:amethyst_block", white, white, white)
                .with_texture(44)
                .with_hardness(1.5)
                .with_roughness(0.20)
                .with_metallic(0.20)
                .with_icon_color([72, 42, 92]),
        );

        // 40. Glowstone
        registry.register(
            BlockDef::solid_tinted("survival:glowstone", white, white, white)
                .with_texture(45)
                .with_hardness(0.4)
                .with_roughness(0.30)
                .with_shading(ShadingModel::Emissive)
                .with_emission([1.0, 0.95, 0.65])
                .with_icon_color([255, 230, 160]),
        );

        // 41. Soul Sand
        registry.register(
            BlockDef::solid_tinted("survival:soul_sand", white, white, white)
                .with_texture(46)
                .with_hardness(0.5)
                .with_roughness(0.95)
                .with_icon_color([62, 44, 27]),
        );

        // 42. Terracotta
        registry.register(
            BlockDef::solid_tinted("survival:terracotta", white, white, white)
                .with_texture(47)
                .with_hardness(1.25)
                .with_roughness(0.90)
                .with_icon_color([114, 90, 70]),
        );

        // 43. Blackstone
        registry.register(
            BlockDef::solid_tinted("survival:blackstone", white, white, white)
                .with_texture(48)
                .with_hardness(2.0)
                .with_roughness(0.85)
                .with_icon_color([34, 34, 35]),
        );

        // 44. Prismarine
        registry.register(
            BlockDef::solid_tinted("survival:prismarine", white, white, white)
                .with_texture(49)
                .with_hardness(1.5)
                .with_roughness(0.70)
                .with_icon_color([75, 127, 110]),
        );

        // 45. Prismarine Bricks
        registry.register(
            BlockDef::solid_tinted("survival:prismarine_bricks", white, white, white)
                .with_texture(50)
                .with_hardness(1.5)
                .with_roughness(0.70)
                .with_icon_color([86, 124, 108]),
        );

        // 46. Nether Bricks
        registry.register(
            BlockDef::solid_tinted("survival:nether_bricks", white, white, white)
                .with_texture(51)
                .with_hardness(2.0)
                .with_roughness(0.80)
                .with_icon_color([37, 18, 19]),
        );

        // 47. Birch Planks
        registry.register(
            BlockDef::solid_tinted("survival:birch_planks", white, white, white)
                .with_texture(52)
                .with_hardness(1.0)
                .with_roughness(0.75)
                .with_icon_color([138, 129, 117]),
        );

        // 48. Spruce Planks
        registry.register(
            BlockDef::solid_tinted("survival:spruce_planks", white, white, white)
                .with_texture(53)
                .with_hardness(1.0)
                .with_roughness(0.75)
                .with_icon_color([86, 46, 20]),
        );

        // 49. White Wool
        registry.register(
            BlockDef::solid_tinted("survival:white_wool", white, white, white)
                .with_texture(54)
                .with_hardness(0.8)
                .with_roughness(0.95)
                .with_icon_color([222, 216, 208]),
        );

        // 50. Blue Wool
        registry.register(
            BlockDef::solid_tinted("survival:blue_wool", white, white, white)
                .with_texture(55)
                .with_hardness(0.8)
                .with_roughness(0.95)
                .with_icon_color([70, 101, 171]),
        );

        // 51. White Concrete
        registry.register(
            BlockDef::solid_tinted("survival:white_concrete", white, white, white)
                .with_texture(56)
                .with_hardness(1.8)
                .with_roughness(0.80)
                .with_icon_color([204, 204, 204]),
        );

        // 52. Black Concrete
        registry.register(
            BlockDef::solid_tinted("survival:black_concrete", white, white, white)
                .with_texture(57)
                .with_hardness(1.8)
                .with_roughness(0.80)
                .with_icon_color([66, 66, 65]),
        );

        // 53. Blue Concrete
        registry.register(
            BlockDef::solid_tinted("survival:blue_concrete", white, white, white)
                .with_texture(58)
                .with_hardness(1.8)
                .with_roughness(0.80)
                .with_icon_color([102, 138, 168]),
        );

        // 54. Red Concrete
        registry.register(
            BlockDef::solid_tinted("survival:red_concrete", white, white, white)
                .with_texture(59)
                .with_hardness(1.8)
                .with_roughness(0.80)
                .with_icon_color([151, 145, 138]),
        );

        // 55. TNT Block
        registry.register(
            BlockDef::solid_tinted("survival:tnt", white, white, white)
                .with_face_textures(61, 60, 61)
                .with_hardness(0.2)
                .with_roughness(0.80)
                .with_icon_color([222, 32, 33]),
        );

        // 56. Carved Pumpkin
        registry.register(
            BlockDef::solid_tinted("survival:carved_pumpkin", white, white, white)
                .with_texture(62)
                .with_hardness(1.0)
                .with_roughness(0.75)
                .with_icon_color([203, 93, 42]),
        );

        // 57. Lamp (warm glowing light)
        registry.register(
            BlockDef::solid_tinted(
                "survival:lamp",
                [0.98, 0.88, 0.55],
                [0.95, 0.82, 0.45],
                [0.90, 0.75, 0.38],
            )
            .with_shading(ShadingModel::Emissive)
            .with_emission([1.0, 0.85, 0.45])
            .with_roughness(0.3)
            .with_icon_color([250, 225, 140]),
        );

        // 58. Deepslate
        registry.register(
            BlockDef::solid_tinted("survival:deepslate", white, white, white)
                .with_custom_face_textures([64, 64, 63, 63, 63, 63])
                .with_hardness(3.5)
                .with_roughness(0.85)
                .with_icon_color([78, 78, 80]),
        );

        // 59. Tuff
        registry.register(
            BlockDef::solid_tinted("survival:tuff", white, white, white)
                .with_texture(65)
                .with_hardness(2.0)
                .with_roughness(0.90)
                .with_icon_color([107, 108, 102]),
        );

        // 60. Calcite
        registry.register(
            BlockDef::solid_tinted("survival:calcite", white, white, white)
                .with_texture(66)
                .with_hardness(1.5)
                .with_roughness(0.75)
                .with_icon_color([223, 224, 220]),
        );

        // 61. Dripstone Block
        registry.register(
            BlockDef::solid_tinted("survival:dripstone", white, white, white)
                .with_texture(67)
                .with_hardness(1.5)
                .with_roughness(0.85)
                .with_icon_color([134, 107, 92]),
        );

        // 62. Basalt
        registry.register(
            BlockDef::solid_tinted("survival:basalt", white, white, white)
                .with_face_textures(69, 68, 69)
                .with_hardness(2.5)
                .with_roughness(0.85)
                .with_icon_color([73, 72, 78]),
        );

        // 63. Magma Block (emissive underground volcanic heat)
        registry.register(
            BlockDef::solid_tinted("survival:magma", white, white, white)
                .with_texture(70)
                .with_hardness(1.0)
                .with_roughness(0.40)
                .with_shading(ShadingModel::Emissive)
                .with_emission([0.95, 0.40, 0.15])
                .with_icon_color([136, 60, 28]),
        );

        // 64. Moss Block
        registry.register(
            BlockDef::solid_tinted("survival:moss", white, white, white)
                .with_texture(71)
                .with_hardness(0.3)
                .with_roughness(0.95)
                .with_icon_color([87, 108, 45]),
        );

        // 65. Deepslate Coal Ore
        registry.register(
            BlockDef::solid_tinted("survival:deepslate_coal_ore", white, white, white)
                .with_texture(72)
                .with_hardness(4.0)
                .with_roughness(0.85)
                .with_icon_color([70, 70, 72]),
        );

        // 66. Deepslate Iron Ore
        registry.register(
            BlockDef::solid_tinted("survival:deepslate_iron_ore", white, white, white)
                .with_texture(73)
                .with_hardness(4.5)
                .with_roughness(0.85)
                .with_icon_color([102, 95, 91]),
        );

        // 67. Deepslate Copper Ore
        registry.register(
            BlockDef::solid_tinted("survival:deepslate_copper_ore", white, white, white)
                .with_texture(74)
                .with_hardness(4.0)
                .with_roughness(0.85)
                .with_icon_color([91, 91, 87]),
        );

        // 68. Deepslate Gold Ore
        registry.register(
            BlockDef::solid_tinted("survival:deepslate_gold_ore", white, white, white)
                .with_texture(75)
                .with_hardness(4.5)
                .with_roughness(0.85)
                .with_icon_color([115, 102, 73]),
        );

        // 69. Deepslate Redstone Ore (glowing)
        registry.register(
            BlockDef::solid_tinted("survival:deepslate_redstone_ore", white, white, white)
                .with_texture(76)
                .with_hardness(4.5)
                .with_roughness(0.40)
                .with_shading(ShadingModel::Emissive)
                .with_emission([0.50, 0.05, 0.05])
                .with_icon_color([102, 70, 71]),
        );

        // 70. Deepslate Lapis Ore
        registry.register(
            BlockDef::solid_tinted("survival:deepslate_lapis_ore", white, white, white)
                .with_texture(77)
                .with_hardness(4.5)
                .with_roughness(0.80)
                .with_icon_color([73, 86, 113]),
        );

        // 71. Deepslate Diamond Ore
        registry.register(
            BlockDef::solid_tinted("survival:deepslate_diamond_ore", white, white, white)
                .with_texture(78)
                .with_hardness(5.0)
                .with_roughness(0.75)
                .with_icon_color([80, 105, 106]),
        );

        // 72. Deepslate Emerald Ore
        registry.register(
            BlockDef::solid_tinted("survival:deepslate_emerald_ore", white, white, white)
                .with_texture(79)
                .with_hardness(5.0)
                .with_roughness(0.80)
                .with_icon_color([72, 99, 81]),
        );

        // 73. Water Block (Shader animated, Fresnel, transparent)
        let mut water_def = BlockDef::solid_tinted(
            "survival:water",
            [0.18, 0.52, 0.88],
            [0.18, 0.52, 0.88],
            [0.18, 0.52, 0.88],
        )
        .with_texture(80)
        .with_hardness(100.0)
        .with_shading(ShadingModel::Water)
        .with_transparency(BlockTransparency::Translucent)
        .with_roughness(0.04)
        .with_icon_color([35, 105, 185]);
        water_def.solid = false;
        water_def.opaque = false;
        registry.register(water_def);

        // 74. Tall Grass (Wild prairie grass, drops plant fibers)
        let mut grass_def = BlockDef::solid_tinted(
            "survival:tall_grass",
            [0.55, 0.85, 0.35],
            [0.55, 0.85, 0.35],
            [0.55, 0.85, 0.35],
        )
        .with_texture(81)
        .with_geometry(BlockGeometry::Cross)
        .with_transparency(BlockTransparency::Cutout)
        .with_shading(ShadingModel::Foliage)
        .with_health(35.0)
        .with_hardness(0.1)
        .with_icon_color([110, 185, 65]);
        grass_def.solid = false;
        grass_def.opaque = false;
        registry.register(grass_def);

        // 75. Shrub / Bush (Dry wild bush, drops sticks & fibers)
        let mut shrub_def = BlockDef::solid_tinted(
            "survival:shrub",
            [0.65, 0.72, 0.42],
            [0.65, 0.72, 0.42],
            [0.65, 0.72, 0.42],
        )
        .with_texture(82)
        .with_geometry(BlockGeometry::Cross)
        .with_transparency(BlockTransparency::Cutout)
        .with_shading(ShadingModel::Foliage)
        .with_health(45.0)
        .with_hardness(0.2)
        .with_icon_color([130, 140, 80]);
        shrub_def.solid = false;
        shrub_def.opaque = false;
        registry.register(shrub_def);

        // 76. Plant Fiber (Harvested raw fiber for crafting)
        registry.register(
            BlockDef::solid_tinted("survival:plant_fiber", [0.72, 0.68, 0.38], [0.72, 0.68, 0.38], [0.72, 0.68, 0.38])
                .with_texture(83)
                .with_health(20.0)
                .with_icon_color([175, 165, 95]),
        );

        // 77. Wooden Stick (Fallen branch / twig for crafting)
        registry.register(
            BlockDef::solid_tinted("survival:stick", [0.55, 0.40, 0.25], [0.55, 0.40, 0.25], [0.55, 0.40, 0.25])
                .with_texture(84)
                .with_health(25.0)
                .with_icon_color([140, 100, 60]),
        );
    }

    fn create_generator(&self, registry: &BlockRegistry) -> Arc<dyn WorldGenerator> {
        if self.generator_type == "Classic" {
            Arc::new(SurvivalWorldGen {
                seed: self.seed,
                grass: registry.find("survival:grass").unwrap_or(BlockId::AIR),
                dirt: registry.find("survival:dirt").unwrap_or(BlockId::AIR),
                stone: registry.find("survival:stone").unwrap_or(BlockId::AIR),
                sand: registry.find("survival:sand").unwrap_or(BlockId::AIR),
                wood: registry.find("survival:wood").unwrap_or(BlockId::AIR),
                leaves: registry.find("survival:leaves").unwrap_or(BlockId::AIR),
            })
        } else {
            Arc::new(LithosphereWorldGen::new(self.seed, registry))
        }
    }

    fn spawn_position(&self) -> Vec3 {
        let gen = LithosphereWorldGen::new(self.seed, &BlockRegistry::new());
        let (surface_y, _) = gen.surface_height(0, 0);
        Vec3::new(0.5, (surface_y + 2).max(10) as f32, 0.5)
    }

    fn default_place_block(&self, registry: &BlockRegistry) -> BlockId {
        registry.find("survival:wood").unwrap_or(BlockId::AIR)
    }
}

pub struct SurvivalWorldGen {
    seed: u32,
    grass: BlockId,
    dirt: BlockId,
    stone: BlockId,
    sand: BlockId,
    wood: BlockId,
    leaves: BlockId,
}

impl SurvivalWorldGen {
    fn surface_height(&self, wx: i32, wz: i32) -> i32 {
        let base = fbm2d(wx as f32 * 0.018, wz as f32 * 0.018, self.seed);
        let detail = fbm2d(wx as f32 * 0.065, wz as f32 * 0.065, self.seed.wrapping_add(137));
        let h = 10.0 + base * 22.0 + detail * 6.0;
        h.round() as i32
    }

    fn has_tree_at(&self, wx: i32, wz: i32) -> Option<i32> {
        let h = hash2(wx, wz, self.seed.wrapping_add(991));
        if h % 137 != 0 {
            return None;
        }
        let ground = self.surface_height(wx, wz);
        if ground < 12 {
            return None;
        }
        Some(ground)
    }
}

impl WorldGenerator for SurvivalWorldGen {
    fn vertical_bounds(&self) -> (i32, i32) {
        (-3, 6)
    }

    fn column_max_height(&self, x: i32, z: i32) -> i32 {
        self.surface_height(x, z) + 12
    }

    fn generate_chunk(&self, chunk_pos: IVec3, chunk_size: usize, out: &mut [BlockId]) {
        let cs = chunk_size as i32;
        let base_x = chunk_pos.x * cs;
        let base_y = chunk_pos.y * cs;
        let base_z = chunk_pos.z * cs;

        for lz in 0..chunk_size {
            let wz = base_z + lz as i32;
            for lx in 0..chunk_size {
                let wx = base_x + lx as i32;
                let height = self.surface_height(wx, wz);

                for ly in 0..chunk_size {
                    let wy = base_y + ly as i32;
                    let idx = lx + lz * chunk_size + ly * chunk_size * chunk_size;

                    if wy > height {
                        continue;
                    }
                    out[idx] = if wy == height {
                        if height <= 8 {
                            self.sand
                        } else {
                            self.grass
                        }
                    } else if wy >= height - 3 {
                        if height <= 8 {
                            self.sand
                        } else {
                            self.dirt
                        }
                    } else {
                        self.stone
                    };
                }
            }
        }

        // Simple tree pass (checks neighboring columns within +/-2 blocks so canopies cross chunk edges cleanly).
        for tz in (base_z - 2)..=(base_z + cs + 1) {
            for tx in (base_x - 2)..=(base_x + cs + 1) {
                let Some(ground) = self.has_tree_at(tx, tz) else {
                    continue;
                };
                let trunk_top = ground + 4;
                for wy in (ground + 1)..=trunk_top {
                    if wy >= base_y && wy < base_y + cs && tx >= base_x && tx < base_x + cs && tz >= base_z && tz < base_z + cs {
                        let lx = (tx - base_x) as usize;
                        let ly = (wy - base_y) as usize;
                        let lz = (tz - base_z) as usize;
                        out[lx + lz * chunk_size + ly * chunk_size * chunk_size] = self.wood;
                    }
                }
                for wy in (trunk_top - 1)..=(trunk_top + 2) {
                    if wy < base_y || wy >= base_y + cs {
                        continue;
                    }
                    let radius: i32 = if wy >= trunk_top + 1 { 1 } else { 2 };
                    for dz in -radius..=radius {
                        for dx in -radius..=radius {
                            if dx == 0 && dz == 0 && wy <= trunk_top {
                                continue;
                            }
                            if dx.abs() == radius && dz.abs() == radius && radius == 2 {
                                continue;
                            }
                            let wx = tx + dx;
                            let wz = tz + dz;
                            if wx >= base_x && wx < base_x + cs && wz >= base_z && wz < base_z + cs {
                                let lx = (wx - base_x) as usize;
                                let ly = (wy - base_y) as usize;
                                let lz = (wz - base_z) as usize;
                                let idx = lx + lz * chunk_size + ly * chunk_size * chunk_size;
                                if out[idx] == BlockId::AIR {
                                    out[idx] = self.leaves;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[inline]
fn hash2(x: i32, z: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(374_761_393)
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
