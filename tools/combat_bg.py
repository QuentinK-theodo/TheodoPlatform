"""Convert Combat-background.webp into a GBA-ready 240x160 background (gfx/combat_bg.png).

Imported as a 16-colour background, with its own palette swapped in during combat.
Run from the repo root:  python3 tools/combat_bg.py
"""
from pathlib import Path

from PIL import Image, ImageEnhance

ROOT = Path(__file__).resolve().parent.parent

src = Image.open(ROOT / "Combat-background.webp").convert("RGB")
# Crop to 3:2, anchored right so the "theodo." logo stays in frame.
w, h = src.size
cw = int(h * 3 / 2)
img = src.crop((w - cw, 0, w, h)).resize((240, 160), Image.LANCZOS)
# Pale, low-contrast backdrop so the fighters stand out.
img = ImageEnhance.Contrast(img).enhance(0.7)
img = Image.blend(img, Image.new("RGB", img.size, (245, 238, 228)), 0.45)

# One 15-colour palette (16-colour tiles): 256-colour tiles don't fit in VRAM
# alongside the map, and agb can't pack per-tile palettes for this image.
# Budget: 11 colours for the pale backdrop, 4 for the foreground floor.
FLOOR_Y = 98  # fighters' feet are at y~114; the UI panel starts at y=120
back = img.quantize(11, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE).convert("RGB")

room = Image.open(ROOT / "Modern tiles_Free/Interiors_free/16x16/Room_Builder_free_16x16.png").convert("RGB")
plank = room.crop((176, 208, 192, 224))  # herringbone parquet
floor = Image.new("RGB", (240, 160 - FLOOR_Y))
for x in range(0, 240, 16):
    for y in range(0, floor.height, 16):
        floor.paste(plank, (x, y))
floor = floor.quantize(3, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE).convert("RGB")
out = back.copy()
out.paste(floor, (0, FLOOR_Y))
# Dark skirting line between wall and floor (15th colour).
for x in range(240):
    out.putpixel((x, FLOOR_Y), (58, 40, 38))
    out.putpixel((x, FLOOR_Y + 1), (58, 40, 38))

# Fighter shadow sprite (16x8 ellipse, one dark colour, transparent elsewhere).
shadow = Image.new("RGBA", (16, 8), (0, 0, 0, 0))
for y in range(8):
    for x in range(16):
        if ((x - 7.5) / 7.5) ** 2 + ((y - 3.5) / 3.0) ** 2 <= 1:
            shadow.putpixel((x, y), (40, 28, 30, 255))
shadow.save(ROOT / "gfx/shadow.png")
out.save(ROOT / "gfx/combat_bg.png")
out.resize((960, 640), Image.NEAREST).save(ROOT / "target/combat_bg_preview.png")
print("ok", len(set(out.getdata())), "colours")
