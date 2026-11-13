pub const TEXTURE_WIDTH: u32 = 64;
pub const TEXTURE_HEIGHT: u32 = 64;

pub const RAW_BLOCK_TEXTURES: &[(&str, &[u8])] = &[
    ("grass_block_top", include_bytes!("../../../assets/textures/blocks/grass_block_top.png")),
    ("grass_block_side", include_bytes!("../../../assets/textures/blocks/grass_block_side.png")),
    ("dirt", include_bytes!("../../../assets/textures/blocks/dirt.png")),
    ("stone", include_bytes!("../../../assets/textures/blocks/stone.png")),
    ("cobblestone", include_bytes!("../../../assets/textures/blocks/cobblestone.png")),
    ("sand", include_bytes!("../../../assets/textures/blocks/sand.png")),
    ("oak_planks", include_bytes!("../../../assets/textures/blocks/oak_planks.png")),
    ("oak_log", include_bytes!("../../../assets/textures/blocks/oak_log.png")),
    ("oak_log_top", include_bytes!("../../../assets/textures/blocks/oak_log_top.png")),
    ("oak_leaves", include_bytes!("../../../assets/textures/blocks/oak_leaves.png")),
    ("glass", include_bytes!("../../../assets/textures/blocks/glass.png")),
    ("bricks", include_bytes!("../../../assets/textures/blocks/bricks.png")),
    ("bookshelf", include_bytes!("../../../assets/textures/blocks/bookshelf.png")),
    ("crafting_table_front", include_bytes!("../../../assets/textures/blocks/crafting_table_front.png")),
    ("crafting_table_side", include_bytes!("../../../assets/textures/blocks/crafting_table_side.png")),
    ("furnace_front", include_bytes!("../../../assets/textures/blocks/furnace_front.png")),
    ("furnace_side", include_bytes!("../../../assets/textures/blocks/furnace_side.png")),
    ("furnace_top", include_bytes!("../../../assets/textures/blocks/furnace_top.png")),
    ("gold_block", include_bytes!("../../../assets/textures/blocks/gold_block.png")),
    ("iron_block", include_bytes!("../../../assets/textures/blocks/iron_block.png")),
    ("diamond_block", include_bytes!("../../../assets/textures/blocks/diamond_block.png")),
    ("diamond_ore", include_bytes!("../../../assets/textures/blocks/diamond_ore.png")),
    ("andesite", include_bytes!("../../../assets/textures/blocks/andesite.png")),
    ("bedrock", include_bytes!("../../../assets/textures/blocks/bedrock.png")),
    ("obsidian", include_bytes!("../../../assets/textures/blocks/obsidian.png")),
    ("stone_bricks", include_bytes!("../../../assets/textures/blocks/stone_bricks.png")),
    ("mossy_cobblestone", include_bytes!("../../../assets/textures/blocks/mossy_cobblestone.png")),
    ("smooth_stone", include_bytes!("../../../assets/textures/blocks/smooth_stone.png")),
    ("gravel", include_bytes!("../../../assets/textures/blocks/gravel.png")),
    ("clay", include_bytes!("../../../assets/textures/blocks/clay.png")),
    ("snow", include_bytes!("../../../assets/textures/blocks/snow.png")),
    ("ice", include_bytes!("../../../assets/textures/blocks/ice.png")),
    ("coal_ore", include_bytes!("../../../assets/textures/blocks/coal_ore.png")),
    ("iron_ore", include_bytes!("../../../assets/textures/blocks/iron_ore.png")),
    ("gold_ore", include_bytes!("../../../assets/textures/blocks/gold_ore.png")),
    ("redstone_ore", include_bytes!("../../../assets/textures/blocks/redstone_ore.png")),
    ("redstone_block", include_bytes!("../../../assets/textures/blocks/redstone_block.png")),
    ("lapis_ore", include_bytes!("../../../assets/textures/blocks/lapis_ore.png")),
    ("lapis_block", include_bytes!("../../../assets/textures/blocks/lapis_block.png")),
    ("emerald_ore", include_bytes!("../../../assets/textures/blocks/emerald_ore.png")),
    ("emerald_block", include_bytes!("../../../assets/textures/blocks/emerald_block.png")),
    ("copper_ore", include_bytes!("../../../assets/textures/blocks/copper_ore.png")),
    ("copper_block", include_bytes!("../../../assets/textures/blocks/copper_block.png")),
    ("amethyst_block", include_bytes!("../../../assets/textures/blocks/amethyst_block.png")),
    ("glowstone", include_bytes!("../../../assets/textures/blocks/glowstone.png")),
    ("soul_sand", include_bytes!("../../../assets/textures/blocks/soul_sand.png")),
    ("terracotta", include_bytes!("../../../assets/textures/blocks/terracotta.png")),
    ("blackstone", include_bytes!("../../../assets/textures/blocks/blackstone.png")),
    ("prismarine", include_bytes!("../../../assets/textures/blocks/prismarine.png")),
    ("prismarine_bricks", include_bytes!("../../../assets/textures/blocks/prismarine_bricks.png")),
    ("nether_bricks", include_bytes!("../../../assets/textures/blocks/nether_bricks.png")),
    ("birch_planks", include_bytes!("../../../assets/textures/blocks/birch_planks.png")),
    ("spruce_planks", include_bytes!("../../../assets/textures/blocks/spruce_planks.png")),
    ("white_wool", include_bytes!("../../../assets/textures/blocks/white_wool.png")),
    ("blue_wool", include_bytes!("../../../assets/textures/blocks/blue_wool.png")),
    ("white_concrete", include_bytes!("../../../assets/textures/blocks/white_concrete.png")),
    ("black_concrete", include_bytes!("../../../assets/textures/blocks/black_concrete.png")),
    ("blue_concrete", include_bytes!("../../../assets/textures/blocks/blue_concrete.png")),
    ("red_concrete", include_bytes!("../../../assets/textures/blocks/red_concrete.png")),
    ("tnt_side", include_bytes!("../../../assets/textures/blocks/tnt_side.png")),
    ("tnt_top", include_bytes!("../../../assets/textures/blocks/tnt_top.png")),
    ("carved_pumpkin", include_bytes!("../../../assets/textures/blocks/carved_pumpkin.png")),
    ("deepslate", include_bytes!("../../../assets/textures/blocks/deepslate.png")),
    ("deepslate_top", include_bytes!("../../../assets/textures/blocks/deepslate_top.png")),
    ("tuff", include_bytes!("../../../assets/textures/blocks/tuff.png")),
    ("calcite", include_bytes!("../../../assets/textures/blocks/calcite.png")),
    ("dripstone_block", include_bytes!("../../../assets/textures/blocks/dripstone_block.png")),
    ("basalt_side", include_bytes!("../../../assets/textures/blocks/basalt_side.png")),
    ("basalt_top", include_bytes!("../../../assets/textures/blocks/basalt_top.png")),
    ("magma", include_bytes!("../../../assets/textures/blocks/magma.png")),
    ("moss_block", include_bytes!("../../../assets/textures/blocks/moss_block.png")),
    ("deepslate_coal_ore", include_bytes!("../../../assets/textures/blocks/deepslate_coal_ore.png")),
    ("deepslate_iron_ore", include_bytes!("../../../assets/textures/blocks/deepslate_iron_ore.png")),
    ("deepslate_copper_ore", include_bytes!("../../../assets/textures/blocks/deepslate_copper_ore.png")),
    ("deepslate_gold_ore", include_bytes!("../../../assets/textures/blocks/deepslate_gold_ore.png")),
    ("deepslate_redstone_ore", include_bytes!("../../../assets/textures/blocks/deepslate_redstone_ore.png")),
    ("deepslate_lapis_ore", include_bytes!("../../../assets/textures/blocks/deepslate_lapis_ore.png")),
    ("deepslate_diamond_ore", include_bytes!("../../../assets/textures/blocks/deepslate_diamond_ore.png")),
    ("deepslate_emerald_ore", include_bytes!("../../../assets/textures/blocks/deepslate_emerald_ore.png")),
    ("water_still", include_bytes!("../../../assets/textures/blocks/water_still.png")),
];

pub fn load_texture_array_data() -> (Vec<u8>, u32) {
    let num_layers = RAW_BLOCK_TEXTURES.len() as u32;
    let layer_size = (TEXTURE_WIDTH * TEXTURE_HEIGHT * 4) as usize;
    let mut buffer = vec![255u8; layer_size * num_layers as usize];

    for (layer_idx, (name, png_bytes)) in RAW_BLOCK_TEXTURES.iter().enumerate() {
        let offset = layer_idx * layer_size;
        let slice = &mut buffer[offset..offset + layer_size];

        match image::load_from_memory(png_bytes) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                for y in 0..TEXTURE_HEIGHT {
                    for x in 0..TEXTURE_WIDTH {
                        let src_x = (x * w / TEXTURE_WIDTH).min(w.saturating_sub(1));
                        let src_y = (y * h / TEXTURE_HEIGHT).min(h.saturating_sub(1));
                        let pixel = rgba.get_pixel(src_x, src_y);
                        let dst_idx = ((y * TEXTURE_WIDTH + x) * 4) as usize;
                        slice[dst_idx..dst_idx + 4].copy_from_slice(&pixel.0);
                    }
                }
            }
            Err(e) => {
                eprintln!("Warning: failed to decode texture {name}: {e}");
            }
        }
    }

    (buffer, num_layers)
}

pub const MIP_LEVEL_COUNT: u32 = 7;

pub fn generate_mipmaps(base_data: &[u8], width: u32, height: u32, layers: u32) -> Vec<Vec<u8>> {
    let mut mips = Vec::with_capacity(MIP_LEVEL_COUNT as usize);
    mips.push(base_data.to_vec());

    let mut prev_w = width;
    let mut prev_h = height;

    for _ in 1..MIP_LEVEL_COUNT {
        let cur_w = (prev_w / 2).max(1);
        let cur_h = (prev_h / 2).max(1);
        let cur_layer_size = (cur_w * cur_h * 4) as usize;
        let prev_layer_size = (prev_w * prev_h * 4) as usize;
        let mut cur_data = vec![0u8; cur_layer_size * layers as usize];

        let prev_data = mips.last().unwrap();

        for layer in 0..layers {
            let prev_offset = layer as usize * prev_layer_size;
            let cur_offset = layer as usize * cur_layer_size;

            for y in 0..cur_h {
                for x in 0..cur_w {
                    let src_x = x * 2;
                    let src_y = y * 2;

                    let mut r = 0u32;
                    let mut g = 0u32;
                    let mut b = 0u32;
                    let mut a = 0u32;
                    let mut samples = 0u32;

                    for dy in 0..2 {
                        for dx in 0..2 {
                            let px = (src_x + dx).min(prev_w.saturating_sub(1));
                            let py = (src_y + dy).min(prev_h.saturating_sub(1));
                            let idx = prev_offset + ((py * prev_w + px) * 4) as usize;
                            r += prev_data[idx] as u32;
                            g += prev_data[idx + 1] as u32;
                            b += prev_data[idx + 2] as u32;
                            a += prev_data[idx + 3] as u32;
                            samples += 1;
                        }
                    }

                    let dst_idx = cur_offset + ((y * cur_w + x) * 4) as usize;
                    cur_data[dst_idx] = (r / samples) as u8;
                    cur_data[dst_idx + 1] = (g / samples) as u8;
                    cur_data[dst_idx + 2] = (b / samples) as u8;
                    cur_data[dst_idx + 3] = (a / samples) as u8;
                }
            }
        }

        mips.push(cur_data);
        prev_w = cur_w;
        prev_h = cur_h;
    }

    mips
}

pub fn create_block_texture_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::TextureView, wgpu::Sampler) {
    let (data, layers) = load_texture_array_data();
    let mips = generate_mipmaps(&data, TEXTURE_WIDTH, TEXTURE_HEIGHT, layers);

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("block-texture-array"),
        size: wgpu::Extent3d {
            width: TEXTURE_WIDTH,
            height: TEXTURE_HEIGHT,
            depth_or_array_layers: layers,
        },
        mip_level_count: MIP_LEVEL_COUNT,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    for (mip_level, mip_data) in mips.iter().enumerate() {
        let mip_w = (TEXTURE_WIDTH >> mip_level).max(1);
        let mip_h = (TEXTURE_HEIGHT >> mip_level).max(1);
        let layer_size = (mip_w * mip_h * 4) as usize;

        for layer in 0..layers {
            let offset = layer as usize * layer_size;
            let layer_bytes = &mip_data[offset..offset + layer_size];
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip_level as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                layer_bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(mip_w * 4),
                    rows_per_image: Some(mip_h),
                },
                wgpu::Extent3d {
                    width: mip_w,
                    height: mip_h,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("block-texture-array-view"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("block-texture-sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    (view, sampler)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_texture_array_data() {
        let (data, layers) = load_texture_array_data();
        assert_eq!(layers, RAW_BLOCK_TEXTURES.len() as u32);
        assert_eq!(data.len(), (TEXTURE_WIDTH * TEXTURE_HEIGHT * 4 * layers) as usize);
        // Verify not all bytes are zero
        assert!(data.iter().any(|&b| b > 0));
    }
}
