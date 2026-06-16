from PIL import Image
import os

textures = [
    "dirt.png",
    "grass_block_top.png",
    "grass_block_side.png",
    "stone.png",
    "sand.png",
    "water_still.png",
    "oak_log_top.png",
    "oak_log.png",
    "oak_leaves.png",
    "bedrock.png",
]

grid_size = 4
tex_size = 16
atlas = Image.new('RGBA', (grid_size * tex_size, grid_size * tex_size), (0, 0, 0, 0))

for idx, tex in enumerate(textures):
    path = os.path.join("assets/textures/block", tex)
    if os.path.exists(path):
        img = Image.open(path).convert("RGBA")
        # Resize if not 16x16
        if img.size != (tex_size, tex_size):
            img = img.resize((tex_size, tex_size), Image.NEAREST)
        x = (idx % grid_size) * tex_size
        y = (idx // grid_size) * tex_size
        atlas.paste(img, (x, y))
    else:
        print(f"Warning: {path} not found")

atlas.save("assets/textures/block/atlas.png")
print("Atlas saved to assets/textures/block/atlas.png")
