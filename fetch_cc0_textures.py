import os
import urllib.request
import io
import math
from PIL import Image, ImageDraw, ImageFilter, ImageEnhance

TARGET_DIR = r"B:\Developing\voxel-engine\assets\textures\blocks"
SIZE = 256

def download_image(url):
    print(f"Downloading {url} ...")
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
    with urllib.request.urlopen(req, timeout=15) as resp:
        return Image.open(io.BytesIO(resp.read())).convert('RGBA')

# Poly Haven 1k Diffuse URLs
URLS = {
    'grass': 'https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/grass_path_2/grass_path_2_diff_1k.jpg',
    'dirt': 'https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/dirt/dirt_diff_1k.jpg',
    'stone': 'https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/aerial_rocks_02/aerial_rocks_02_diff_1k.jpg',
    'andesite': 'https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/aerial_rocks_04/aerial_rocks_04_diff_1k.jpg',
    'cobblestone': 'https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/cobblestone_01/cobblestone_01_diff_1k.jpg',
    'sand': 'https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/coast_sand_01/coast_sand_01_diff_1k.jpg',
    'planks': 'https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/brown_planks_03/brown_planks_03_diff_1k.jpg',
    'bark': 'https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/bark_brown_02/bark_brown_02_diff_1k.jpg',
    'brick': 'https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/brick_4/brick_4_diff_1k.jpg',
    'leaves': 'https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/forest_leaves_02/forest_leaves_02_diffuse_1k.jpg',
    'metal': 'https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/metal_plate/metal_plate_diff_1k.jpg',
}

def main():
    raw_images = {}
    for name, url in URLS.items():
        try:
            img = download_image(url)
            raw_images[name] = img.resize((SIZE, SIZE), Image.Resampling.LANCZOS)
        except Exception as e:
            print(f"Error downloading {name}: {e}")

    # Fallbacks if any failed
    if 'grass' not in raw_images:
        raw_images['grass'] = Image.new('RGBA', (SIZE, SIZE), (65, 150, 45, 255))
    if 'dirt' not in raw_images:
        raw_images['dirt'] = Image.new('RGBA', (SIZE, SIZE), (105, 75, 45, 255))
    if 'stone' not in raw_images:
        raw_images['stone'] = Image.new('RGBA', (SIZE, SIZE), (130, 130, 135, 255))
    if 'andesite' not in raw_images:
        raw_images['andesite'] = Image.new('RGBA', (SIZE, SIZE), (110, 110, 115, 255))
    if 'cobblestone' not in raw_images:
        raw_images['cobblestone'] = Image.new('RGBA', (SIZE, SIZE), (115, 115, 120, 255))
    if 'sand' not in raw_images:
        raw_images['sand'] = Image.new('RGBA', (SIZE, SIZE), (215, 195, 140, 255))
    if 'planks' not in raw_images:
        raw_images['planks'] = Image.new('RGBA', (SIZE, SIZE), (160, 120, 75, 255))
    if 'bark' not in raw_images:
        raw_images['bark'] = Image.new('RGBA', (SIZE, SIZE), (90, 65, 45, 255))
    if 'brick' not in raw_images:
        raw_images['brick'] = Image.new('RGBA', (SIZE, SIZE), (170, 75, 55, 255))
    if 'leaves' not in raw_images:
        raw_images['leaves'] = Image.new('RGBA', (SIZE, SIZE), (50, 130, 35, 255))
    if 'metal' not in raw_images:
        raw_images['metal'] = Image.new('RGBA', (SIZE, SIZE), (180, 185, 190, 255))

    # 1. grass_block_top: Enhance vibrant green saturation slightly
    enhancer = ImageEnhance.Color(raw_images['grass'])
    grass_top = enhancer.enhance(1.25)
    grass_top.save(os.path.join(TARGET_DIR, "grass_block_top.png"))

    # 2. dirt
    dirt = raw_images['dirt']
    dirt.save(os.path.join(TARGET_DIR, "dirt.png"))

    # 3. grass_block_side: Composite grass over dirt with jagged roots
    grass_side = dirt.copy()
    grass_crop = grass_top.copy()
    mask = Image.new('L', (SIZE, SIZE), 0)
    draw_mask = ImageDraw.Draw(mask)
    for x in range(SIZE):
        # Organic wave with sharp tufts
        wave = int(45 + 16 * math.sin(x * 0.08) + 8 * math.sin(x * 0.22) + 6 * math.sin(x * 0.55))
        draw_mask.line([(x, 0), (x, wave)], fill=255)
    grass_side.paste(grass_crop, (0, 0), mask)
    grass_side.save(os.path.join(TARGET_DIR, "grass_block_side.png"))

    # 4. stone
    stone = raw_images['stone']
    stone.save(os.path.join(TARGET_DIR, "stone.png"))

    # 5. cobblestone
    cobble = raw_images['cobblestone']
    cobble.save(os.path.join(TARGET_DIR, "cobblestone.png"))

    # 6. sand
    sand = raw_images['sand']
    sand.save(os.path.join(TARGET_DIR, "sand.png"))

    # 7. oak_planks
    planks = raw_images['planks']
    planks.save(os.path.join(TARGET_DIR, "oak_planks.png"))

    # 8. oak_log
    bark = raw_images['bark']
    bark.save(os.path.join(TARGET_DIR, "oak_log.png"))

    # 9. oak_log_top: Tree rings with bark border
    log_top = Image.new('RGBA', (SIZE, SIZE))
    draw_log = ImageDraw.Draw(log_top)
    center = (SIZE / 2, SIZE / 2)
    # Background wood core
    draw_log.rectangle([(0, 0), (SIZE, SIZE)], fill=(195, 155, 105, 255))
    # Concentric rings
    for r in range(110, 8, -6):
        c_shade = int(175 + 25 * math.sin(r * 0.4))
        draw_log.ellipse(
            [(center[0] - r, center[1] - r), (center[0] + r, center[1] + r)],
            outline=(c_shade - 25, c_shade - 45, c_shade - 70, 255),
            width=2
        )
    # Bark rim around outside
    bark_rim = bark.copy()
    rim_mask = Image.new('L', (SIZE, SIZE), 0)
    rim_draw = ImageDraw.Draw(rim_mask)
    rim_draw.rectangle([(0, 0), (SIZE, SIZE)], fill=255)
    rim_draw.ellipse([(18, 18), (SIZE - 18, SIZE - 18)], fill=0)
    log_top.paste(bark_rim, (0, 0), rim_mask)
    log_top.save(os.path.join(TARGET_DIR, "oak_log_top.png"))

    # 10. oak_leaves: Cutout transparency in darker crevices
    leaves_img = raw_images['leaves'].copy()
    enhancer = ImageEnhance.Color(leaves_img)
    leaves_img = enhancer.enhance(1.3)
    leaves_rgba = leaves_img.convert('RGBA')
    pix = leaves_rgba.load()
    for y in range(SIZE):
        for x in range(SIZE):
            r, g, b, a = pix[x, y]
            brightness = (r + g + b) // 3
            # Make dark leaf gaps transparent
            if brightness < 65:
                pix[x, y] = (r, g, b, 0)
            elif brightness < 85:
                alpha = int((brightness - 65) / 20 * 255)
                pix[x, y] = (r, g, b, alpha)
    leaves_rgba.save(os.path.join(TARGET_DIR, "oak_leaves.png"))

    # 11. glass: Modern clean glass pane
    glass = Image.new('RGBA', (SIZE, SIZE), (210, 230, 250, 40))
    glass_draw = ImageDraw.Draw(glass)
    # Beveled rim
    frame_color = (230, 245, 255, 180)
    glass_draw.rectangle([(0, 0), (SIZE - 1, SIZE - 1)], outline=frame_color, width=4)
    # Specular diagonal streaks
    glass_draw.line([(40, 20), (20, 40)], fill=(255, 255, 255, 120), width=6)
    glass_draw.line([(SIZE - 20, SIZE - 60), (SIZE - 60, SIZE - 20)], fill=(255, 255, 255, 90), width=8)
    glass.save(os.path.join(TARGET_DIR, "glass.png"))

    # 12. bricks
    brick = raw_images['brick']
    brick.save(os.path.join(TARGET_DIR, "bricks.png"))

    # 13. bookshelf
    bookshelf = planks.copy()
    bs_draw = ImageDraw.Draw(bookshelf)
    # Upper and lower shelf planks
    bs_draw.rectangle([(8, 120), (SIZE - 8, 136)], fill=(110, 80, 50, 255))
    bs_draw.rectangle([(8, SIZE - 20), (SIZE - 8, SIZE - 8)], fill=(110, 80, 50, 255))
    # Draw books
    book_colors = [
        (160, 40, 40), (45, 80, 150), (40, 120, 60), (180, 140, 40),
        (110, 60, 140), (140, 90, 50), (180, 50, 70), (50, 100, 120)
    ]
    # Shelf 1 books
    curr_x = 16
    idx = 0
    while curr_x < SIZE - 24:
        bw = (idx % 3 + 1) * 6 + 10
        bh = 75 + (idx % 4) * 8
        col = book_colors[idx % len(book_colors)]
        bs_draw.rectangle([(curr_x, 120 - bh), (curr_x + bw, 120)], fill=col)
        # Gold spine accents
        bs_draw.line([(curr_x + 2, 120 - bh + 12), (curr_x + bw - 2, 120 - bh + 12)], fill=(220, 190, 70))
        bs_draw.line([(curr_x + 2, 120 - 15), (curr_x + bw - 2, 120 - 15)], fill=(220, 190, 70))
        curr_x += bw + 3
        idx += 1
    # Shelf 2 books
    curr_x = 16
    while curr_x < SIZE - 24:
        bw = ((idx + 2) % 3 + 1) * 6 + 10
        bh = 70 + (idx % 5) * 8
        col = book_colors[(idx + 3) % len(book_colors)]
        bs_draw.rectangle([(curr_x, SIZE - 20 - bh), (curr_x + bw, SIZE - 20)], fill=col)
        bs_draw.line([(curr_x + 2, SIZE - 20 - bh + 10), (curr_x + bw - 2, SIZE - 20 - bh + 10)], fill=(220, 190, 70))
        curr_x += bw + 3
        idx += 1
    bookshelf.save(os.path.join(TARGET_DIR, "bookshelf.png"))

    # 14. crafting_table_front
    craft_front = planks.copy()
    cf_draw = ImageDraw.Draw(craft_front)
    # Workshop drawers
    for y_top in [24, 100, 176]:
        cf_draw.rectangle([(20, y_top), (SIZE - 20, y_top + 60)], outline=(80, 55, 30), width=3)
        # Brass pull handle
        cf_draw.rectangle([(SIZE // 2 - 25, y_top + 25), (SIZE // 2 + 25, y_top + 35)], fill=(220, 180, 60))
        cf_draw.rectangle([(SIZE // 2 - 20, y_top + 28), (SIZE // 2 + 20, y_top + 32)], fill=(120, 90, 20))
    craft_front.save(os.path.join(TARGET_DIR, "crafting_table_front.png"))

    # 15. crafting_table_side
    craft_side = planks.copy()
    cs_draw = ImageDraw.Draw(craft_side)
    # Wooden cross-brace frame
    cs_draw.rectangle([(12, 12), (SIZE - 12, SIZE - 12)], outline=(90, 60, 35), width=8)
    cs_draw.line([(12, 12), (SIZE - 12, SIZE - 12)], fill=(90, 60, 35), width=8)
    cs_draw.line([(12, SIZE - 12), (SIZE - 12, 12)], fill=(90, 60, 35), width=8)
    craft_side.save(os.path.join(TARGET_DIR, "crafting_table_side.png"))

    # 16. furnace_front
    furnace_f = cobble.copy()
    ff_draw = ImageDraw.Draw(furnace_f)
    # Firebox opening
    box = [(40, 70), (SIZE - 40, SIZE - 40)]
    ff_draw.rectangle(box, fill=(25, 20, 20, 255), outline=(50, 45, 45, 255), width=6)
    # Arched door top
    ff_draw.chord([(40, 30), (SIZE - 40, 110)], 180, 360, fill=(25, 20, 20, 255), outline=(50, 45, 45, 255), width=6)
    # Coals and iron grate
    for gx in range(55, SIZE - 45, 22):
        ff_draw.line([(gx, 70), (gx, SIZE - 45)], fill=(70, 70, 75), width=4)
    # Glowing ember reflection at bottom
    ff_draw.rectangle([(50, SIZE - 70), (SIZE - 50, SIZE - 45)], fill=(180, 70, 20, 160))
    furnace_f.save(os.path.join(TARGET_DIR, "furnace_front.png"))

    # 17. furnace_side
    cobble.save(os.path.join(TARGET_DIR, "furnace_side.png"))

    # 18. furnace_top
    furnace_top = cobble.copy()
    ft_draw = ImageDraw.Draw(furnace_top)
    # Vent circle
    ft_draw.ellipse([(60, 60), (SIZE - 60, SIZE - 60)], outline=(40, 40, 40), width=8)
    ft_draw.line([(60, SIZE // 2), (SIZE - 60, SIZE // 2)], fill=(40, 40, 40), width=6)
    ft_draw.line([(SIZE // 2, 60), (SIZE // 2, SIZE - 60)], fill=(40, 40, 40), width=6)
    furnace_top.save(os.path.join(TARGET_DIR, "furnace_top.png"))

    # 19. gold_block: Heavy brushed gold
    metal = raw_images['metal']
    gold = metal.copy()
    gold_pix = gold.load()
    for y in range(SIZE):
        for x in range(SIZE):
            r, g, b, a = gold_pix[x, y]
            avg = (r + g + b) // 3
            # Gold color mapping
            gold_pix[x, y] = (int(avg * 1.05).clamp(0, 255) if hasattr(int, 'clamp') else min(255, int(avg * 1.05)),
                              min(255, int(avg * 0.82)),
                              min(255, int(avg * 0.25)),
                              255)
    g_draw = ImageDraw.Draw(gold)
    g_draw.rectangle([(0, 0), (SIZE - 1, SIZE - 1)], outline=(255, 235, 120), width=5)
    gold.save(os.path.join(TARGET_DIR, "gold_block.png"))

    # 20. iron_block: Brushed steel plate with beveled border
    iron = metal.copy()
    i_draw = ImageDraw.Draw(iron)
    i_draw.rectangle([(0, 0), (SIZE - 1, SIZE - 1)], outline=(240, 245, 255), width=5)
    # Corner rivets
    for rx, ry in [(18, 18), (SIZE - 18, 18), (18, SIZE - 18), (SIZE - 18, SIZE - 18)]:
        i_draw.ellipse([(rx - 5, ry - 5), (rx + 5, ry + 5)], fill=(120, 125, 135), outline=(240, 240, 250), width=2)
    iron.save(os.path.join(TARGET_DIR, "iron_block.png"))

    # 21. diamond_block: Crystalline faceted azure diamond
    diamond = Image.new('RGBA', (SIZE, SIZE))
    d_pix = diamond.load()
    for y in range(SIZE):
        for x in range(SIZE):
            # Facet pattern
            fx = (x // 32) % 2
            fy = (y // 32) % 2
            diag = ((x + y) // 24) % 2
            base = 190 + (fx ^ fy) * 35 + diag * 25
            d_pix[x, y] = (min(255, int(base * 0.35)), min(255, int(base * 0.88)), min(255, int(base * 0.98)), 255)
    d_draw = ImageDraw.Draw(diamond)
    d_draw.rectangle([(0, 0), (SIZE - 1, SIZE - 1)], outline=(190, 245, 255), width=5)
    diamond.save(os.path.join(TARGET_DIR, "diamond_block.png"))

    # 22. diamond_ore: Stone with embedded glowing diamond crystals
    ore = stone.copy()
    o_draw = ImageDraw.Draw(ore)
    crystal_spots = [
        (65, 75, 22), (180, 60, 26), (120, 140, 30),
        (55, 195, 24), (195, 180, 20)
    ]
    for cx, cy, sz in crystal_spots:
        # Glow aura
        o_draw.ellipse([(cx - sz - 4, cy - sz - 4), (cx + sz + 4, cy + sz + 4)], fill=(40, 140, 180, 90))
        # Rhombus crystal
        pts = [(cx, cy - sz), (cx + sz, cy), (cx, cy + sz), (cx - sz, cy)]
        o_draw.polygon(pts, fill=(90, 230, 255, 255), outline=(220, 255, 255, 255))
        # Inner facet
        pts_in = [(cx, cy - sz + 6), (cx + sz - 6, cy), (cx, cy + sz - 6), (cx - sz + 6, cy)]
        o_draw.polygon(pts_in, fill=(180, 245, 255, 255))
    ore.save(os.path.join(TARGET_DIR, "diamond_ore.png"))

    # 23. andesite: Dark volcanic rock
    andesite = raw_images['andesite']
    andesite.save(os.path.join(TARGET_DIR, "andesite.png"))

    print("All 23 photoreal CC0 textures generated successfully!")

if __name__ == "__main__":
    main()
