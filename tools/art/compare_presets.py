"""Side-by-side of preset v1 ('A (atual)') vs v1-vivid ('B (vivo)') at 64 pt and 32 pt (@2x pixels), light and dark."""
import sys, json, pathlib, zlib
sys.path.insert(0, str(pathlib.Path(__file__).parent))
import numpy as np
from PIL import Image, ImageDraw, ImageFont
import zoen_style as z
from build_set import circle, ROOT, seed_of

AG = ["agent-furball-wave", "agent-coffee-cup", "agent-fox-traveler", "agent-octopus-coder"]
GR = ["group-party-balloons", "group-beach-trip", "group-camping-tent", "group-family-dinner"]
F = lambda s, b=False: ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans%s.ttf" % ("-Bold" if b else ""), s)
S64, S32 = 128, 64                       # 64 pt and 32 pt at @2x
LIGHT, DARK = (255, 255, 255), (0, 0, 0)

def vivid(slug):
    kind = slug.split("-")[0]
    cache = ROOT.parent / "_scratch" / f"vivid-{slug}.png"
    if not cache.exists():
        fr, _ = z.stylize(Image.open(ROOT / "_raw/flux" / f"{slug}.webp"), kind=kind, seed=seed_of(slug), frames=1, crop=0.985, preset="v1-vivid")
        fr[0].save(cache)
    return Image.open(cache).convert("RGB")

def main(out):
    pair_w = 2 * (S64 + 28); col_w = pair_w + 40
    block_h = 60 + 2 * (S64 + 30) + 2 * (S32 + 30) + 20
    Wd = 170 + 4 * col_w + 30; H = 150 + 2 * block_h + 70
    img = Image.new("RGB", (Wd, H), (243, 237, 225)); d = ImageDraw.Draw(img)
    d.text((30, 26), "Zoen · A (atual) vs B (vivo)", font=F(40, True), fill=(43, 38, 34))
    d.text((30, 82), "A = preset v1 (set entregue) · B = preset v1-vivid (mais cor + aguada pastel atrás) · tamanhos reais @2x: 64 pt = 128 px, 32 pt = 64 px",
           font=F(19), fill=(100, 92, 84))
    for bi, ids in enumerate((AG, GR)):
        y0 = 150 + bi * block_h
        d.text((30, y0 + 8), "Agentes" if bi == 0 else "Grupos", font=F(24, True), fill=(43, 38, 34))
        rows = [("claro · 64 pt", LIGHT, S64, False), ("claro · 32 pt", LIGHT, S32, False),
                ("escuro · 64 pt", DARK, S64, True), ("escuro · 32 pt", DARK, S32, True)]
        y = y0 + 60
        ys = []
        for lab, bg, s, dark in rows:
            h = s + 30
            d.rectangle((160, y, Wd - 30, y + h - 6), fill=bg)
            d.text((30, y + h / 2 - 14), lab, font=F(18), fill=(70, 64, 58))
            ys.append((y, h, s, dark, bg)); y += h
        for ci, slug in enumerate(ids):
            x0 = 170 + ci * col_w
            d.text((x0, y0 + 14), slug.split("-", 1)[1], font=F(18, True), fill=(60, 54, 48))
            a = Image.open(ROOT / slug.split("-")[0] / f"{slug}.png").convert("RGB"); b = vivid(slug)
            for (yy, h, s, dark, bg) in ys:
                for k, (im, tag) in enumerate(((a, "A (atual)"), (b, "B (vivo)"))):
                    src = z.dark_variant(im) if dark else im
                    cx = x0 + k * (S64 + 28) + (S64 - s) // 2
                    c = circle(src, s, dark=dark)
                    img.paste(c, (cx, yy + 4), c)
                    if s == S64:
                        d.text((x0 + k * (S64 + 28) + 18, yy + h - 26 + 0), tag, font=F(13), fill=(200, 200, 200) if dark else (90, 84, 78))
    img.save(out, optimize=True)

if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else str(ROOT.parent / "compare-v1-vs-vivid.png"))
