# Zoen art pipeline

This turns any base image into a Zoen hand-drawn asset: ink line art and watercolour washes on warm paper,
with a "line boil" loop (three hand-redrawn-looking frames at 10 fps). It built `../avatars-v1/`
(24 agents, 22 groups). The server art service behind the in-app **Gerar** button will reuse it
(see `../design-art-generation.md`).

```
base image (any model, any size) ──► zoen_style.stylize() ──► 3 frames + colours ──► export_asset()
                                        │                                               ├─ <slug>.png         1024 still
  1. crop/normalise (drop watermark)    │                                               ├─ <slug>@512.png     512 still
  2. subject mask (paper flood-fill)    │                                               ├─ <slug>-dark@512.png dark-mode still
  3. circle-safe fit, or organic        │                                               ├─ <slug>.webp        animated loop 384 (primary)
     vignette for scenes / cut edges    │                                               ├─ <slug>.apng        animated loop 256 (fallback)
  4. mean-shift + k-means → wash areas  │                                               ├─ <slug>.mp4         H.264 loop 384 (fallback)
  5. palette unification (hue pull,     │                                               └─ <slug>.json        colours, seed, preset, files
     chroma cap, translucent lift)      │
  6. ink: edges + silhouette → skeleton │
     → variable-width stroke, dry-brush │
  7. washes: wobbly edges, rim          │
     darkening, granulation, misreg.    │
  8. shared procedural paper            │
  9. boil: per-frame smooth displacement│
     of ink + wash, re-skeletonised     │
```

## Quick start

```bash
python3 -m venv .venv && .venv/bin/pip install -r pipeline/requirements.txt   # plus ffmpeg with libx264 on PATH
.venv/bin/python pipeline/zoen_art.py base.webp --kind agent --slug agent-fox-traveler --out out/
.venv/bin/python pipeline/zoen_art.py scenes/*.png --kind group --out out/ --manifest out/manifest.json
```

Options:

| Flag | Default | Meaning |
|---|---|---|
| `--kind agent\|group` | required | `agent` fits an object in the circle-safe disc. `group` also allows scenes; anything that runs off the canvas is painted into an organic vignette, so straight canvas edges never appear |
| `--slug` | `<kind>-<stem>` | output name |
| `--seed` | `crc32(slug) % 10000` | the boil, the wash jitter and the misregistration are all seeded, so a given input always gives the same output |
| `--crop` | `0.985` | fraction of the source kept. Use `0.90` for watermarked providers such as Pollinations |
| `--preset` | `v1-vivid` | **`v1-vivid` has been the default since 2026-10-08.** The subject's own hues are kept exactly. Dark washes such as eyes and pupils stay neutral, and small dark shapes become solid ink. A pastel watercolour disc in a palette hue from the harmonious (split-complementary) band is painted around the subject, never over it. The whole avatars-v1 set is rendered with it. `v1` is the earlier muted look, kept bit-reproducible, and its outputs are backed up in `../_backup/avatars-v1-preset-v1/` |
| `--frames` | `3` | boil frames |
| `--manifest` | none | upserts entries into a manifest.json |

Python API: `stylize(img, kind, seed, frames, k, crop, preset) -> (frames, meta)` and
`export_asset(frames, meta, outdir, slug) -> files`. It runs on CPU at about 12–15 s per asset (3 frames at
1024 working resolution, one core), and needs no GPU or network.

## Contract the app relies on

- **Circle-safe.** In object mode the subject radius is ≤ 0.43 (agents) or 0.44 (groups) of the canvas width,
  centred, so a circle crop never clips it.
- **Sizes.** Use the still for lists and Reduce Motion, and animate only in large views (profile, chat
  header). Animated WebP is the primary format; ImageIO and `CGAnimateImageAtURLWithBlock` decode it on iOS 14+.
  - Measured on avatars-v1 (46 assets), animated WebP at 384 px is 28–65 KB (43 KB on average), and the MP4 39–59 KB.
  - APNG at 256 px with 128 colours is 132–153 KB.
  - Animated HEIC was skipped: ffmpeg can't mux HEIF image sequences, and WebP covers the need.
- **Colours** (`meta.colors`):
  - `dominant`: the largest wash.
  - `accent`: the most saturated wash.
  - `tint`: the dominant colour deepened, for UI tinting.
  - Known weakness: when the subject is mostly white or cream (cup, calendar), `dominant`/`accent` come out
    paper-coloured. Use `tint` in the UI.
- **Dark mode.** `-dark@512.png` dims the paper to warm stone so the disc doesn't glare on black.
- **`STYLE_VERSION`** (`zoen-ink-wash/1`) is part of every cache key in the generation service. Bump it
  whenever the same inputs would render differently.

## Files

| File | What |
|---|---|
| `zoen_style.py` | the reusable style module (no I/O except exports) |
| `zoen_art.py` | **the single CLI entry point** (base image in, stills, loops and metadata out) |
| `build_set.py` | builds the whole avatars-v1 set from `jobs.json`, plus contact sheets and the grid video |
| `jobs.json` | the 48 subjects, tags and prompts behind avatars-v1 |
| `horde.py`, `horde_batch.py` | AI Horde (FLUX.1-schnell) client, anonymous, async submit and poll. **Offline asset work only, never for user requests** (LAION prompt sharing and the profit-share clause). Staging and production providers are in `../design-art-generation.md` §4 |
| `package_xcode.py` | builds `../avatars-v1/xcode/AvatarV1/` (asset catalog with 64 pt 1x/2x/3x stills, WebP heroes and loops, Manifest.json) |
| `compare_presets.py` | renders `../compare-v1-vs-vivid.png` (A = v1, B = v1-vivid, 64 and 32 pt, light and dark) |
| `gen_base.py` | provider adapter: Pollinations (model `sana`), anonymous. Tried first and dropped: rate-limited (HTTP 402), weaker style, watermark |

## Provenance of avatars-v1 base art

- **Provider:** AI Horde (aihorde.net), anonymous key `0000000000`. It's a free, crowdsourced, volunteer GPU network.
- **Model:** `Flux.1-Schnell fp8 (Compact)`, 512×512, 4 steps.
- **Prompts:** in `jobs.json`. The style suffix is in `horde_batch.py`.
- **Re-rendering:** every asset was re-rendered by `zoen_style` preset `v1`.
- **Rejected:** two subjects came back censored three times (paint palette, camera) and were dropped. The
  Pollinations/`sana` bases in `_raw/base` were never used in the delivered set.
- **Licence and terms:** see `../design-art-generation.md` §11.
