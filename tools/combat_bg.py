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
img = ImageEnhance.Brightness(img).enhance(0.85)
img = ImageEnhance.Contrast(img).enhance(1.1)

# One 15-colour palette (16-colour tiles): 256-colour tiles don't fit in VRAM
# alongside the map, and agb can't pack per-tile palettes for this image.
out = img.quantize(15, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE).convert("RGB")
out.save(ROOT / "gfx/combat_bg.png")
out.resize((960, 640), Image.NEAREST).save(ROOT / "target/combat_bg_preview.png")
print("ok", len(set(out.getdata())), "colours")
