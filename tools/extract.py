"""Extract the GBA-ready subset of LimeZu's Modern Interiors (free) pack into gfx/.

Run from the repo root:  python3 tools/extract.py
The raw pack lives in "Modern tiles_Free/" (git-ignored).
"""
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
PACK = ROOT / "Modern tiles_Free"
OUT = ROOT / "gfx"
OUT.mkdir(exist_ok=True)

ROOM = Image.open(PACK / "Interiors_free/16x16/Room_Builder_free_16x16.png").convert("RGBA")
INTERIOR = Image.open(PACK / "Interiors_free/16x16/Interiors_free_16x16.png").convert("RGBA")

# Order must match `Terrain` in src/wfc.rs.
# (sheet, x, y) in pixels, 16x16 each. Objects are composited over the floor.
FLOOR = (ROOM, 176, 112)
TILES = [
    ("floor", [FLOOR]),
    ("carpet", [(ROOM, 176, 208)]),
    ("tile", [(ROOM, 176, 144)]),
    ("wall", [(ROOM, 128, 96)]),
    ("box", [FLOOR, (INTERIOR, 176, 168)]),
    ("stool", [FLOOR, (INTERIOR, 96, 212)]),
]


def crop(sheet, x, y, w=16, h=16):
    return sheet.crop((x, y, x + w, y + h))


def quantize(img: Image.Image, colours: int = 15) -> Image.Image:
    """Reduce to <= colours opaque colours, keeping full transparency."""
    alpha = img.getchannel("A").point(lambda a: 255 if a > 127 else 0)
    rgb = img.convert("RGB").quantize(colours, method=Image.Quantize.MEDIANCUT).convert("RGB")
    out = rgb.convert("RGBA")
    out.putalpha(alpha)
    return out


tiles = Image.new("RGBA", (16 * len(TILES), 16), (0, 0, 0, 255))
for i, (_, layers) in enumerate(TILES):
    t = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
    for sheet, x, y in layers:
        t.alpha_composite(crop(sheet, x, y))
    bg = Image.new("RGBA", (16, 16), (0, 0, 0, 255))
    bg.alpha_composite(t)
    tiles.paste(quantize(bg), (16 * i, 0))
tiles.save(OUT / "tiles.png")

# Characters: run sheets are 24 frames of 16x32 (right, up, left, down x6),
# idle sheets are 4 frames (right, up, left, down).
DIRS = ["right", "up", "left", "down"]


def save_gifs(anims):
    """anims: {path: [frames]}. All frames share one 15-colour palette so the
    whole character fits a single GBA sprite palette."""
    allf = [f for fs in anims.values() for f in fs]
    strip = Image.new("RGBA", (16 * len(allf), 32), (0, 0, 0, 0))
    for i, f in enumerate(allf):
        strip.alpha_composite(f, (16 * i, 0))
    strip = quantize(strip)
    i = 0
    for path, fs in anims.items():
        out = [strip.crop((16 * (i + k), 0, 16 * (i + k) + 16, 32)) for k in range(len(fs))]
        i += len(fs)
        out[0].save(path, save_all=True, append_images=out[1:], duration=100,
                    loop=0, disposal=2, optimize=False)


chars = PACK / "Characters_free"
for who in ["Adam", "Bob", "Alex"]:
    anims = {}
    if who == "Adam":
        run = Image.open(chars / "Adam_run_16x16.png").convert("RGBA")
        for d, name in enumerate(DIRS):
            anims[OUT / f"adam_{name}.gif"] = [crop(run, (d * 6 + i) * 16, 0, 16, 32) for i in range(6)]
    idle = Image.open(chars / f"{who}_idle_anim_16x16.png").convert("RGBA")
    # idle_anim: 24 frames, down-facing are the last 6
    anims[OUT / f"{who.lower()}_idle.gif"] = [crop(idle, (18 + i) * 16, 0, 16, 32) for i in range(6)]
    save_gifs(anims)

print("ok:", sorted(p.name for p in OUT.iterdir()))
