"""Convert Combat-background.webp into a GBA-ready 240x160 background (gfx/combat_bg.png).

Imported as a 256-colour background, with its own palette swapped in during combat.
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

# 256-colour background: one global palette. Keep < 240 colours so palette
# bank 15 stays free for the UI (bar / panels).
out = img.quantize(200, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE).convert("RGB")
out.save(ROOT / "gfx/combat_bg.png")
out.resize((960, 640), Image.NEAREST).save(ROOT / "target/combat_bg_preview.png")
print("ok", len(set(out.getdata())), "colours")
