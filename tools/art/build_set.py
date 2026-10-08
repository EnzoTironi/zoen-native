"""Build the Zoen default art set: stylize every base in _raw/base, export, write manifest, contact sheets, grid video."""
import sys, json, zlib, pathlib, subprocess, tempfile, time
sys.path.insert(0, str(pathlib.Path(__file__).parent))
import numpy as np
from PIL import Image, ImageDraw, ImageFont, ImageFilter
import zoen_style as z

ROOT = pathlib.Path("/workspace/zoen-assets/avatars-v1")
RAW = ROOT / "_raw/base"
JOBS = json.load(open(pathlib.Path(__file__).parent / "jobs.json"))
PREFER_SANA = set(json.load(open(ROOT / "_prefer_sana.json"))) if (ROOT / "_prefer_sana.json").exists() else set()
CUT = set(json.load(open(ROOT / "_cut.json"))) if (ROOT / "_cut.json").exists() else set()
FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf"
FONT_R = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"

def seed_of(slug): return zlib.crc32(slug.encode()) % 10000

def _render(args):
    jid, kind, src, seed, crop = args
    t = time.time()
    frames, meta = z.stylize(Image.open(src), kind=kind, seed=seed, preset=z.DEFAULT_PRESET, crop=crop)
    files = z.export_asset(frames, meta, ROOT / kind, jid)
    return jid, meta, files, time.time() - t

def build(only=None, workers=int(__import__("os").environ.get("ZOEN_WORKERS", "1"))):
    man = json.load(open(ROOT / "manifest.json")) if (ROOT / "manifest.json").exists() else {"assets": []}
    by = {a["id"]: a for a in man["assets"]}
    todo = []
    for j in JOBS:
        if only and j["id"] not in only: continue
        flux = ROOT / "_raw/flux" / f'{j["id"]}.webp'
        src = flux if (flux.exists() and j["id"] not in PREFER_SANA) else RAW / f'{j["id"]}.jpg'
        if not src.exists(): continue
        prov = ("aihorde", "Flux.1-Schnell fp8") if src.suffix == ".webp" else ("pollinations", "sana")
        if not only and j["id"] in by and by[j["id"]].get("base") == src.name and (ROOT / j["kind"] / f'{j["id"]}.png').exists(): continue
        todo.append((j, src, prov))
    from multiprocessing import Pool
    args = [(j["id"], j["kind"], str(src), seed_of(j["id"]), 0.985 if src.suffix == ".webp" else 0.90) for j, src, prov in todo]
    jmap = {j["id"]: (j, src, prov) for j, src, prov in todo}
    with Pool(max(1, workers)) as pool:
      for jid, meta, files, dt in pool.imap_unordered(_render, args):
        j, src, prov = jmap[jid]
        by[j["id"]] = {"id": j["id"], "kind": j["kind"], "tags": j["tags"], "prompt": j["subject"],
                       "files": {k: f'{j["kind"]}/{v}' for k, v in files.items()},
                       "colors": {k: meta[k] for k in ("dominant", "accent", "tint", "backdrop") if k in meta},
                       "preset": meta["preset"],
                       "style": meta["style"], "seed": meta["seed"], "boil": {"frames": z.BOIL_FRAMES, "fps": z.BOIL_FPS},
                       "source": {"provider": prov[0], "model": prov[1]}, "base": src.name}
        print(j["id"], round(dt, 1), "s", flush=True)
    order = [j["id"] for j in JOBS]
    assets = [by[i] for i in order if i in by]
    for a in assets: a["status"] = "cut" if a["id"] in CUT else "keep"
    man = {"version": "avatars-v1", "style": f"{z.STYLE_VERSION}+{z.DEFAULT_PRESET}", "preset": z.DEFAULT_PRESET,
           "notes": "Hand-drawn set for AGENTS and GROUPS/Spaces only. People use their own photos; no-photo fallback is a monogram (UI).",
           "circleSafe": {"agent": 0.43, "group": 0.44, "unit": "subject radius / canvas width"},
           "assets": assets}
    json.dump(man, open(ROOT / "manifest.json", "w"), indent=2)
    return man

# ------------------------------------------------------------------ sheets
def circle(img, d, ring=True, dark=False):
    big = img.resize((d * 3, d * 3), Image.LANCZOS)
    m = Image.new("L", (d * 3, d * 3), 0); ImageDraw.Draw(m).ellipse((0, 0, d * 3 - 1, d * 3 - 1), fill=255)
    out = Image.new("RGBA", (d * 3, d * 3)); out.paste(big, (0, 0), m)
    if ring:
        ImageDraw.Draw(out).ellipse((1, 1, d * 3 - 2, d * 3 - 2), outline=(43, 38, 34, 60 if not dark else 90), width=3)
    return out.resize((d, d), Image.LANCZOS)

def sheet(man, kind, title, path):
    items = [a for a in man["assets"] if a["kind"] == kind and a["status"] == "keep"]
    cols = 6; big = 220; cw, ch = 300, 330
    rows = (len(items) + cols - 1) // cols
    Wd = cols * cw + 80; strip_h = 2 * 150 + 60
    H = 170 + rows * ch + strip_h + 60
    pap, _ = z.paper(1024)
    bg = Image.fromarray((pap * 255).astype(np.uint8)).resize((max(Wd, H),) * 2, Image.LANCZOS).crop((0, 0, Wd, H)).convert("RGBA")
    d = ImageDraw.Draw(bg)
    f1, f2, f3 = ImageFont.truetype(FONT, 46), ImageFont.truetype(FONT_R, 17), ImageFont.truetype(FONT_R, 22)
    d.text((40, 40), title, font=f1, fill=(43, 38, 34))
    d.text((40, 104), f"{len(items)} assets · ink + watercolour on paper · large = 220 px · list = 64 px (32 pt @2x) and 32 px (32 pt @1x)", font=f3, fill=(110, 100, 90))
    for i, a in enumerate(items):
        im = Image.open(ROOT / a["files"]["still1024"]).convert("RGB")
        x, y = 40 + (i % cols) * cw, 170 + (i // cols) * ch
        bg.alpha_composite(circle(im, big), (x, y))
        bg.alpha_composite(circle(im, 64), (x + big + 8, y + 20))
        bg.alpha_composite(circle(im, 32), (x + big + 24, y + 100))
        name = a["id"].split("-", 1)[1]
        d.text((x + 4, y + big + 14), name, font=f2, fill=(60, 54, 48))
        sw = a["colors"]["tint"]
        d.rounded_rectangle((x + big + 18, y + 150, x + big + 52, y + 168), 6, fill=sw)
    # dark-mode strip
    y0 = 170 + rows * ch + 10
    for k, (bgc, lab) in enumerate((((0, 0, 0), "dark · #000"), ((28, 28, 30), "dark · #1C1C1E"))):
        yy = y0 + k * 150
        d.rounded_rectangle((30, yy, Wd - 30, yy + 135), 22, fill=bgc)
        d.text((50, yy + 10), lab, font=f2, fill=(200, 200, 200))
        x = 50
        for a in items:
            im = Image.open(ROOT / a["files"]["stillDark512"]).convert("RGB")
            if x + 70 > Wd - 40: break
            bg.alpha_composite(circle(im, 64, dark=True), (x, yy + 36))
            bg.alpha_composite(circle(im, 32, dark=True), (x + 16, yy + 104 - 2)) if False else None
            x += 74
        # a list-size row (32 px) under it
        x = 50
        for a in items:
            im = Image.open(ROOT / a["files"]["stillDark512"]).convert("RGB")
            if x + 36 > Wd - 40: break
            bg.alpha_composite(circle(im, 32, dark=True), (x, yy + 104 - 2 + 0))
            x += 38
    bg.convert("RGB").save(path, optimize=True)

def grid_video(man, path, cols=7, cell=150, seconds=4, fps=30):
    items = [a for a in man["assets"] if a["status"] == "keep"]
    rows = (len(items) + cols - 1) // cols
    pap, _ = z.paper(1024)
    base = Image.fromarray((pap * 255).astype(np.uint8)).resize((cols * cell + 40, cols * cell + 40)).crop((0, 0, cols * cell + 40, rows * cell + 40)).convert("RGBA")
    # load the 3 boil frames from the animated webp
    anims = []
    for a in items:
        w = Image.open(ROOT / a["files"]["animWebP"]); fr = []
        for i in range(w.n_frames):
            w.seek(i); fr.append(circle(w.convert("RGB"), cell - 14))
        anims.append(fr)
    with tempfile.TemporaryDirectory() as td:
        n = seconds * fps
        for t in range(n):
            img = base.copy()
            for i, fr in enumerate(anims):
                k = (int(t * 10 / fps) + i) % len(fr)          # 10 fps boil, phase-offset per asset
                img.alpha_composite(fr[k], (20 + (i % cols) * cell + 7, 20 + (i // cols) * cell + 7))
            img.convert("RGB").save(f"{td}/{t:04d}.png")
        subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-framerate", str(fps), "-i", f"{td}/%04d.png", "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2",
                        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "26", "-preset", "slow", "-movflags", "+faststart", path], check=True)

if __name__ == "__main__":
    args = sys.argv[1:]
    man = build(set(args) if args and args[0] != "sheets" else None)
    if "sheets" in args or not args:
        sheet(man, "agent", "Zoen · agent avatars v1", str(ROOT / "sheet-agents.png"))
        sheet(man, "group", "Zoen · group & Space photos v1", str(ROOT / "sheet-groups.png"))
        grid_video(man, str(ROOT / "loops-grid.mp4"))
        print("sheets done")
