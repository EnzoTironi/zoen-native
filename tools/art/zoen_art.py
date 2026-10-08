#!/usr/bin/env python3
"""zoen_art: single CLI entry for the Zoen art style.

Base image(s) in -> Zoen-styled stills + boil loops + metadata out.

  python zoen_art.py BASE.png --kind agent --out OUT/ [--slug agent-fox] [--seed N] [--crop 0.985]
                     [--preset v1|v1-vivid] [--frames 3] [--tags a,b] [--manifest OUT/manifest.json]

Per input it writes into OUT/:
  <slug>.png            still 1024x1024 (frame 0, list/reduce-motion)
  <slug>@512.png        still 512
  <slug>-dark@512.png   still for dark mode (paper dimmed to warm stone)
  <slug>.webp           animated WebP loop, 384 px, 3 frames @ 10 fps (primary animated format)
  <slug>.apng           animated PNG loop, 256 px (fallback)
  <slug>.mp4            H.264 loop, 384 px (fallback / video players)
  <slug>.json           metadata: colours (dominant/accent/tint), seed, preset, style version, files
and, with --manifest, upserts the entry into a manifest.json (same schema as avatars-v1/manifest.json).
"""
import argparse, json, pathlib, sys, time, zlib
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from PIL import Image
import zoen_style as z

def default_seed(slug: str) -> int:
    return zlib.crc32(slug.encode()) % 10000          # same rule build_set.py used for avatars-v1

def run_one(src, kind, out, slug=None, seed=None, crop=0.985, preset=z.DEFAULT_PRESET, frames=z.BOIL_FRAMES, tags=()):
    src = pathlib.Path(src)
    slug = slug or f"{kind}-{src.stem}"
    seed = default_seed(slug) if seed is None else seed
    t = time.time()
    fr, meta = z.stylize(Image.open(src), kind=kind, seed=seed, frames=frames, crop=crop, preset=preset)
    files = z.export_asset(fr, meta, out, slug)
    entry = {"id": slug, "kind": kind, "tags": list(tags), "files": files,
             "colors": {k: meta[k] for k in ("dominant", "accent", "tint") if k in meta},
             "style": meta["style"], "preset": meta["preset"], "seed": seed, "crop": crop,
             "boil": {"frames": frames, "fps": z.BOIL_FPS}, "base": src.name}
    if "backdrop" in meta: entry["colors"]["backdrop"] = meta["backdrop"]
    (pathlib.Path(out) / f"{slug}.json").write_text(json.dumps(entry, indent=2))
    return entry, time.time() - t

def upsert_manifest(path, entries):
    path = pathlib.Path(path)
    man = json.loads(path.read_text()) if path.exists() else {"version": "custom", "style": z.STYLE_VERSION, "assets": []}
    by = {a["id"]: a for a in man["assets"]}
    for e in entries:
        prev = by.get(e["id"], {})
        # paths are stored relative to the manifest's folder when outputs live in <manifest dir>/<kind>/
        rel = {k: (f'{e["kind"]}/{v}' if (path.parent / e["kind"] / v).exists() else v) for k, v in e["files"].items()}
        by[e["id"]] = {**prev, **e, "files": rel}
    man["assets"] = list(by.values())
    path.write_text(json.dumps(man, indent=2))

def main(argv=None):
    ap = argparse.ArgumentParser(description="Apply the Zoen ink + watercolour style to base images.")
    ap.add_argument("inputs", nargs="+", help="base image(s): png/jpg/webp, any size, ideally a subject on a plain light background")
    ap.add_argument("--kind", choices=["agent", "group"], required=True)
    ap.add_argument("--out", required=True, help="output directory")
    ap.add_argument("--slug", help="output name (single input only); default <kind>-<file stem>")
    ap.add_argument("--seed", type=int, help="default: crc32(slug) %% 10000 (stable per asset)")
    ap.add_argument("--crop", type=float, default=0.985, help="fraction of source kept; 0.90 trims watermarks (default 0.985)")
    ap.add_argument("--preset", default=z.DEFAULT_PRESET, choices=sorted(z.PRESETS))
    ap.add_argument("--frames", type=int, default=z.BOIL_FRAMES, help="boil frames (default 3)")
    ap.add_argument("--tags", default="", help="comma-separated tags for metadata")
    ap.add_argument("--manifest", help="upsert results into this manifest.json")
    a = ap.parse_args(argv)
    if a.slug and len(a.inputs) > 1: ap.error("--slug needs exactly one input")
    entries = []
    for src in a.inputs:
        e, dt = run_one(src, a.kind, a.out, a.slug, a.seed, a.crop, a.preset, a.frames, [t for t in a.tags.split(",") if t])
        entries.append(e); print(f'{e["id"]}  {dt:.1f}s  tint {e["colors"].get("tint")}  -> {a.out}', flush=True)
    if a.manifest: upsert_manifest(a.manifest, entries)

if __name__ == "__main__":
    main()
