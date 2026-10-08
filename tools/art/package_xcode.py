"""Package avatars-v1 into an Xcode-ready folder: avatars-v1/xcode/AvatarV1/.

  AvatarV1/
    Manifest.json                  app-facing manifest (names, tags, colours, files)
    README.md
    AvatarV1.xcassets/
      Contents.json
      AgentFurballWave.imageset/   64 pt still: 1x 64 px, 2x 128, 3x 192 (thinned per device by the App Store)
      ...
    Heroes/
      AgentFurballWave.webp        1024 px hero still, lossy WebP (asset catalogs don't take WebP)
    Loops/
      AgentFurballWave.webp        animated loop, 192 px, 3 frames @ 10 fps

Dark mode is applied at runtime (multiply by DARK_MULTIPLY, same as zoen_style.dark_variant) instead of
shipping a second set of stills: saves ~2.5 MB.

Names: CamelCase of the slug (agent-furball-wave -> AgentFurballWave).
PNGs are 8-bit palette (256 colours, Floyd-Steinberg) to keep the app small.
"""
from __future__ import annotations
import json, pathlib, shutil, re
from PIL import Image
import zoen_style as z

ROOT = pathlib.Path("/workspace/zoen-assets/avatars-v1")
OUT = ROOT / "xcode" / "AvatarV1"
SIZES = {"1x": 64, "2x": 128, "3x": 192}     # 64 pt display size
HERO = 1024
LOOP = 192                                    # animated WebP size (profile / chat header)

def camel(slug: str) -> str:
    return "".join(p.capitalize() for p in slug.split("-"))

def write_png8(src: Image.Image, path: pathlib.Path, size: int):
    im = src.resize((size, size), Image.LANCZOS)
    # keep alpha free: RGB. Quantize to 256 with Floyd-Steinberg for paper grain
    q = im.convert("RGB").quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.FLOYDSTEINBERG)
    q.save(path, optimize=True)

DARK = [{"appearance": "luminosity", "value": "dark"}]
PROPS = {"template-rendering-intent": "original"}

DARK_MULTIPLY = (0.86, 0.84, 0.80)
HERO_Q = 72

def imageset_contents(name: str) -> dict:
    imgs = [{"filename": f"{name}{'' if sc == '1x' else '@' + sc}.png", "idiom": "universal", "scale": sc} for sc in ("1x", "2x", "3x")]
    return {"images": imgs, "info": {"author": "zoen", "version": 1}, "properties": PROPS}

README = """# AvatarV1: Zoen default agent and group pictures

Preset `v1-vivid` (style `zoen-ink-wash/1+v1-vivid`): ink and watercolour on paper, with a pastel wash behind each subject.
There are 24 agents and 22 groups. `Manifest.json` is the source of truth for names, tags, colours and files.

## Add to the app
1. Drag `AvatarV1.xcassets` into the app target (or merge its imagesets into the main catalog).
2. Add `Heroes/` and `Loops/` as **folder references** (blue folders) so the paths in `Manifest.json` resolve
   against `Bundle.main`.
3. Bundle `Manifest.json`.

## Use
- **Lists and Reduce Motion:** `Image(asset.files.still)`, a 64 pt still with 1x/2x/3x. Clip it to a circle.
  The art is circle-safe, so no subject is ever cropped.
- **Large views (profile, Space header):** `UIImage(contentsOfFile: Bundle.main.path(forResource: "Heroes/<Name>", ofType: "webp"))`.
  ImageIO decodes WebP on iOS 14+.
- **Animation:** `CGAnimateImageAtURLWithBlock` on `Loops/<Name>.webp`. It's 192 px, 3 frames at 10 fps,
  looping forever. Only animate when `accessibilityReduceMotion` is off, and only where the avatar is
  56 pt or larger.
- **Dark mode:** multiply by `darkModeMultiply` (0.86, 0.84, 0.80) so the paper doesn't glare:
  `.colorMultiply(Color(red: 0.86, green: 0.84, blue: 0.80))` when `colorScheme == .dark`.
- **Tinting:** use `colors.tint` for chips, rings and placeholders, and `colors.backdrop` to match the pastel
  wash. `dominant` and `accent` can come out paper-coloured on white subjects.

## Xcode compression
The imageset PNGs are 8-bit palette files. actool re-encodes images into `Assets.car`, so check the built
size. If it grows, set the imagesets' *Compression* to **Lossy** in the Attributes inspector.

## People
This set is for **agents** and **groups/Spaces** only. People use their own photos, and a person with no
photo gets a monogram (`design-art-generation.md` §9). Never generate faces for people.
"""

def package():
    man = json.load(open(ROOT / "manifest.json"))
    if OUT.exists(): shutil.rmtree(OUT)
    assets = OUT / "AvatarV1.xcassets"; loops = OUT / "Loops"; heroes = OUT / "Heroes"
    assets.mkdir(parents=True); loops.mkdir(); heroes.mkdir()
    (assets / "Contents.json").write_text(json.dumps({"info": {"author": "zoen", "version": 1}}, indent=2))
    packed = []
    for a in man["assets"]:
        if a.get("status") == "cut": continue
        name = camel(a["id"]); iset = assets / f"{name}.imageset"; iset.mkdir()
        still = Image.open(ROOT / a["files"]["still1024"]).convert("RGB")
        for sc, px in SIZES.items():
            write_png8(still, iset / f"{name}{'' if sc == '1x' else '@' + sc}.png", px)
        (iset / "Contents.json").write_text(json.dumps(imageset_contents(name), indent=2))
        still.resize((HERO, HERO), Image.LANCZOS).save(heroes / f"{name}.webp", quality=HERO_Q, method=6)
        # loop: re-encode the existing animated WebP down to LOOP px (and drop the 4th+ frame if any)
        w = Image.open(ROOT / a["files"]["animWebP"]); fr = []
        for i in range(w.n_frames):
            w.seek(i); fr.append(w.convert("RGB").resize((LOOP, LOOP), Image.LANCZOS))
        out = loops / f"{name}.webp"
        fr[0].save(out, save_all=True, append_images=fr[1:], duration=int(1000 / z.BOIL_FPS), loop=0, quality=72, method=6)
        files = {"still": name, "hero": f"Heroes/{name}.webp", "loop": f"Loops/{name}.webp"}   # catalog name + bundle paths
        packed.append({**{k: a[k] for k in ("id", "kind", "tags", "colors", "boil", "preset", "style", "seed") if k in a},
                       "name": name, "files": files})
    out_man = {"version": man["version"], "preset": man.get("preset", z.DEFAULT_PRESET), "style": man["style"],
               "displayPointSize": 64, "scales": SIZES, "heroPx": HERO, "loopPx": LOOP,
               "darkModeMultiply": DARK_MULTIPLY, "loopFps": z.BOIL_FPS, "loopFrames": z.BOIL_FRAMES,
               "circleSafe": man["circleSafe"], "notes": man.get("notes", ""), "assets": packed}
    (OUT / "Manifest.json").write_text(json.dumps(out_man, indent=2))
    (OUT / "README.md").write_text(README)
    # size report
    total = sum(f.stat().st_size for f in OUT.rglob("*") if f.is_file())
    print(f"packed {len(packed)} assets → {OUT}  ({total/1024/1024:.2f} MB)", flush=True)
    return total

if __name__ == "__main__":
    package()
