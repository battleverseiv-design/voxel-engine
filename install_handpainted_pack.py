import os
import urllib.request
import io
from PIL import Image

TARGET_DIR = r"B:\Developing\voxel-engine\assets\textures\blocks"
BASE_URL = "https://gitlab.com/drummyfish/minetest-texturepack/-/raw/master/Hand%20Painted%20Pack%20by%20Drummyfish"

MAPPINGS = {
    "grass_block_top.png": "default_grass.png",
    "grass_block_side.png": "default_grass_side.png",
    "dirt.png": "default_dirt.png",
    "stone.png": "default_stone.png",
    "cobblestone.png": "default_cobble.png",
    "sand.png": "default_sand.png",
    "oak_planks.png": "default_wood.png",
    "oak_log.png": "default_tree.png",
    "oak_log_top.png": "default_tree_top.png",
    "oak_leaves.png": "default_leaves.png",
    "glass.png": "default_glass.png",
    "bricks.png": "default_brick.png",
    "bookshelf.png": "default_bookshelf.png",
    "crafting_table_front.png": "default_chest_front.png",
    "crafting_table_side.png": "default_chest_side.png",
    "furnace_front.png": "default_furnace_front.png",
    "furnace_side.png": "default_furnace_side.png",
    "furnace_top.png": "default_furnace_top.png",
    "gold_block.png": "default_gold_block.png",
    "iron_block.png": "default_steel_block.png",
    "diamond_block.png": "default_diamond_block.png",
    "diamond_ore.png": "default_mineral_diamond.png",
    "andesite.png": "default_desert_stone.png",
}

def main():
    os.makedirs(TARGET_DIR, exist_ok=True)
    for dst_name, src_name in MAPPINGS.items():
        url = f"{BASE_URL}/{src_name}"
        print(f"Downloading {src_name} -> {dst_name} ...")
        req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
        with urllib.request.urlopen(req, timeout=10) as resp:
            data = resp.read()
            img = Image.open(io.BytesIO(data)).convert('RGBA')
            if img.size != (128, 128):
                img = img.resize((128, 128), Image.Resampling.LANCZOS)
            dst_path = os.path.join(TARGET_DIR, dst_name)
            img.save(dst_path)
            print(f"  Saved {dst_name} ({img.size})")

    print("All 23 Hand Painted Pack textures installed successfully!")

if __name__ == "__main__":
    main()
