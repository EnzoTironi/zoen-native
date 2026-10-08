"""
zoen_style: turn ANY base image into a Zoen hand-drawn asset (ink line art + watercolour washes on warm paper),
with a line-boil animation and every export the app needs. Provider-agnostic: the base image is only a source of
shapes and colours; ink, washes, paper, palette and boil are re-rendered here, so any image model gives the same family look.

    from zoen_style import stylize, export_asset
    frames, meta = stylize(Image.open("base.webp"), kind="agent", seed=7, crop=0.985)   # preset="v1"
    files = export_asset(frames, meta, "out/", "agent-fox-traveler")

CLI: see zoen_art.py (single entry point) and README.md.
Deterministic for a given (image, kind, seed, crop, preset). Pure CPU (numpy/opencv/scipy/scikit-image), ~12 s per asset
with 3 boil frames at 1024 working resolution on one core.
"""
from __future__ import annotations
import json, subprocess, tempfile, pathlib, colorsys
import numpy as np, cv2
from PIL import Image
from scipy import ndimage as ndi
from skimage.morphology import skeletonize, remove_small_objects
from dataclasses import dataclass, asdict

STYLE_VERSION = "zoen-ink-wash/1"   # bump when output changes for the same inputs (cache keys depend on it)
W = 1024                      # working resolution
INK = np.array([43, 38, 34], np.float32) / 255      # warm sepia-black ink
PAPER = np.array([243, 237, 225], np.float32) / 255 # warm paper
# Zoen palette anchors (hue unification targets): mascot green, sage, terracotta, ochre, sky, rose, plum, sand
PALETTE = ["#7FAF6B", "#A8C3A0", "#D9825B", "#E3B55B", "#8DB7D4", "#E5A4A0", "#9B7FB0", "#D8C3A0", "#6F9FA8"]
BOIL_FRAMES, BOIL_FPS = 3, 10

@dataclass(frozen=True)
class Preset:
    name: str
    hue_pull: float        # 0..1, how far washes snap toward the nearest palette hue
    chroma_k: float        # saturation multiplier for washes
    chroma_max: float      # saturation ceiling
    wash_l_min: float      # lightness floor (watercolour is translucent)
    agent_radius: float    # subject radius / canvas, object mode (circle-safe margin = 0.5 - this)
    group_radius: float
    backdrop: bool         # pastel watercolour disc painted around the subject
    tint_l: float          # UI tint lightness / saturation rule
    tint_s_add: float
    tint_s_max: float
    dark_neutral_l: float = 0.0   # source lightness below which a wash goes neutral warm grey (eyes, pupils): never tinted
    pupil_frac: float = 0.006     # dark shapes smaller than this fraction of the canvas become solid ink
    backdrop_band: tuple = (0.0, 0.5)  # allowed hue distance (0..0.5) between backdrop and the subject's main hues

PRESETS = {
    # exactly what built avatars-v1 (the delivered sheets)
    "v1": Preset("v1", 0.35, 0.80, 0.62, 0.42, 0.43, 0.44, False, 0.45, 0.10, 1.0),
    # experimental: more colour + pastel backdrop disc (better at 32 pt), not yet reviewed by Enzo
    # default since 2026-10-08: subject hues kept exactly (hue_pull 0), dark washes neutral, solid-ink pupils,
    # pastel backdrop in a harmonious (split-complementary band) palette hue
    "v1-vivid": Preset("v1-vivid", 0.0, 1.05, 0.72, 0.30, 0.43, 0.44, True, 0.50, 0.0, 0.5,
                       dark_neutral_l=0.30, pupil_frac=0.012, backdrop_band=(0.12, 0.36)),
}
DEFAULT_PRESET = "v1-vivid"

def _hex(h): return np.array([int(h[i:i+2], 16) for i in (1, 3, 5)], np.float32) / 255

# ---------------------------------------------------------------- noise & paper
def _noise(shape, sigma, rng, amp=1.0):
    n = ndi.gaussian_filter(rng.standard_normal(shape).astype(np.float32), sigma, mode="wrap")
    return n / (n.std() + 1e-6) * amp

_PAPER_CACHE = {}
def paper(size=W, seed=2026, tone=PAPER):
    key = (size, seed, tuple(np.round(tone, 4)))
    if key in _PAPER_CACHE: return _PAPER_CACHE[key]
    rng = np.random.default_rng(seed)
    s = (size, size)
    mottle = _noise(s, size / 10, rng, 0.018) + _noise(s, size / 40, rng, 0.010)
    grain = _noise(s, 0.8, rng, 0.022) + _noise(s, 2.2, rng, 0.012)
    fib = np.zeros(s, np.float32)
    for _ in range(int(size * 0.9)):              # paper fibres
        x, y = rng.integers(0, size, 2); a = rng.uniform(0, np.pi); l = rng.uniform(4, 18) * size / 1024
        cv2.line(fib, (int(x), int(y)), (int(x + l * np.cos(a)), int(y + l * np.sin(a))), float(rng.uniform(.3, 1)), 1, cv2.LINE_AA)
    fib = cv2.GaussianBlur(fib, (0, 0), 0.6) * 0.035
    tex = 1 + mottle + grain - fib
    img = np.clip(tone[None, None, :] * tex[..., None], 0, 1)
    vign = 1 - 0.035 * (np.hypot(*np.meshgrid(np.linspace(-1, 1, size), np.linspace(-1, 1, size))) ** 2)
    img = img * vign[..., None]
    _PAPER_CACHE[key] = (img.astype(np.float32), (grain + mottle).astype(np.float32))
    return _PAPER_CACHE[key]

# ---------------------------------------------------------------- base prep
def _prep(im: Image.Image, crop=0.90):
    """crop<1 trims canvas borders (provider watermarks); use ~0.98 for clean sources."""
    im = im.convert("RGB"); w, h = im.size; s = min(w, h)
    c = int(s * crop)                             # drop borders (provider watermarks, frame edges)
    l, t = (w - c) // 2, (h - c) // 2 - int(s * (1 - crop) * 0.25)
    t = max(0, t)
    im = im.crop((l, t, l + c, t + c)).resize((W, W), Image.LANCZOS)
    return np.asarray(im).astype(np.float32) / 255

def _subject_mask(rgb):
    """Foreground = not connected-to-border near-paper colour."""
    lab = cv2.cvtColor((rgb * 255).astype(np.uint8), cv2.COLOR_RGB2LAB).astype(np.float32)
    b = np.concatenate([lab[:8].reshape(-1, 3), lab[-8:].reshape(-1, 3), lab[:, :8].reshape(-1, 3), lab[:, -8:].reshape(-1, 3)])
    bg = np.median(b, 0)
    d = np.linalg.norm(lab - bg, axis=2)
    near = (d < 16).astype(np.uint8)
    n, lbl = cv2.connectedComponents(near)
    border = np.unique(np.concatenate([lbl[0], lbl[-1], lbl[:, 0], lbl[:, -1]]))
    bgmask = np.isin(lbl, border[border > 0]) & (near > 0)
    fg = ~bgmask
    fg = cv2.morphologyEx(fg.astype(np.uint8), cv2.MORPH_OPEN, np.ones((5, 5), np.uint8))
    fg = remove_small_objects(fg.astype(bool), max_size=600)
    lab, n = ndi.label(fg)                       # drop stray specks, signatures, floating dots far from the subject
    if n > 1:
        sizes = ndi.sum(fg, lab, range(1, n + 1))
        fg = np.isin(lab, 1 + np.nonzero(sizes >= 0.06 * sizes.max())[0])
    fg = ndi.binary_fill_holes(fg)
    return fg, bg

def _border_touch(fg):
    b = np.concatenate([fg[2], fg[-3], fg[:, 2], fg[:, -3]])
    return float(b.mean())

def _blob(rng, radius, wobble=0.035, soft=26.0):
    """Organic hand-painted disc: radius wobbles with low-frequency noise; returns soft alpha (1 inside)."""
    yy, xx = np.mgrid[0:W, 0:W].astype(np.float32)
    ang = np.arctan2(yy - W / 2, xx - W / 2); rr = np.hypot(yy - W / 2, xx - W / 2)
    ph = rng.uniform(0, 2 * np.pi, 4)
    rad = radius * (1 + wobble * (np.sin(2 * ang + ph[0]) + 0.6 * np.sin(3 * ang + ph[1]) + 0.4 * np.sin(5 * ang + ph[2]) + 0.25 * np.sin(9 * ang + ph[3])))
    rad = rad + _noise((W, W), 6, rng, 4.0)
    return np.clip((rad - rr) / soft + 0.5, 0, 1).astype(np.float32)

def _recenter(rgb, fg, kind, rng, P):
    """Object mode: fit subject in a circle-safe disc. Scene mode (subject runs off the canvas): crop into an
    organic watercolour vignette instead, so no straight canvas edges ever survive."""
    ys, xs = np.nonzero(fg)
    if len(xs) < 500: return rgb, fg, None
    scene = _border_touch(fg) > 0.10 and kind == "group"
    if scene:
        cx, cy = xs.mean() * 0.5 + W / 4, ys.mean() * 0.5 + W / 4
        sc = 1.08
    else:
        x0, x1, y0, y1 = np.percentile(xs, 0.5), np.percentile(xs, 99.5), np.percentile(ys, 0.5), np.percentile(ys, 99.5)
        cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
        r = np.percentile(np.sqrt((xs - cx) ** 2 + (ys - cy) ** 2), 99.7)
        target = W * (P.agent_radius if kind == "agent" else P.group_radius)
        sc = np.clip(target / max(r, 1), 0.55, 1.6)
    M = np.float32([[sc, 0, W / 2 - sc * cx], [0, sc, W / 2 - sc * cy]])
    rgb2 = cv2.warpAffine(rgb, M, (W, W), flags=cv2.INTER_CUBIC, borderMode=cv2.BORDER_REPLICATE)
    fg2 = cv2.warpAffine(fg.astype(np.uint8), M, (W, W), flags=cv2.INTER_NEAREST, borderValue=0) > 0
    vig = _blob(rng, W * 0.43) if scene else None
    if scene:
        fg2 = fg2 & (vig > 0.02)
    else:
        # subject cut by the source canvas: fade it out softly where the old canvas edge lands (no ruler-straight cuts)
        ramp = np.ones((W, W), np.float32)
        r = int(W * 0.06)
        lin = np.linspace(0, 1, r, dtype=np.float32) ** 0.8
        ramp[:r] *= lin[:, None]; ramp[-r:] *= lin[::-1, None]; ramp[:, :r] *= lin[None]; ramp[:, -r:] *= lin[None, ::-1]
        valid = cv2.warpAffine(ramp, M, (W, W), flags=cv2.INTER_LINEAR, borderValue=0)
        if (valid[fg2] < 0.99).mean() > 0.002:
            vig = np.clip(valid + 0.15 * _noise((W, W), 8, rng) * (valid < 0.99), 0, 1)
    return rgb2, fg2, vig

# ---------------------------------------------------------------- colour
def _harmonize(rgb, P):
    """Pull a colour toward the Zoen palette: partial hue snap, gentler chroma, watercolour lightness."""
    h, l, s = colorsys.rgb_to_hls(*rgb)
    if l < P.dark_neutral_l:                              # eyes/pupils/dark accents: neutral warm grey, never hue-shifted
        return np.array(colorsys.hls_to_rgb(0.08, P.wash_l_min + 0.6 * l, 0.06), np.float32)
    best, bd = None, 9
    for p in PALETTE:
        ph, pl, ps = colorsys.rgb_to_hls(*_hex(p))
        d = min(abs(ph - h), 1 - abs(ph - h))
        if d < bd: bd, best = d, ph
    if s > 0.12:
        dh = best - h
        if dh > .5: dh -= 1
        if dh < -.5: dh += 1
        h = (h + P.hue_pull * dh) % 1
    if s < 0.12: h, s = 0.10, 0.10 + s * 0.5           # neutrals -> warm, never cold grey
    s = min(s * P.chroma_k, P.chroma_max)
    l = P.wash_l_min + (1 - P.wash_l_min) * l ** 0.9          # watercolour is translucent: lift values
    l = min(l, 0.93)
    return np.array(colorsys.hls_to_rgb(h, l, s), np.float32)

def _quantize(rgb, fg, k=7, seed=0):
    u8 = (rgb * 255).astype(np.uint8)
    sm = cv2.pyrMeanShiftFiltering(cv2.resize(u8, (W // 2, W // 2)), 10, 22)
    sm = cv2.resize(sm, (W, W), interpolation=cv2.INTER_LINEAR)
    lab = cv2.cvtColor(sm, cv2.COLOR_RGB2LAB).reshape(-1, 3).astype(np.float32)
    pts = lab[fg.reshape(-1)]
    if len(pts) < k: return np.zeros(fg.shape, int) - 1, np.zeros((0, 3)), sm
    cv2.setRNGSeed(seed)
    _, lbl, cent = cv2.kmeans(pts[:: max(1, len(pts) // 60000)], k, None,
                              (cv2.TERM_CRITERIA_EPS + cv2.TERM_CRITERIA_MAX_ITER, 30, 0.5), 3, cv2.KMEANS_PP_CENTERS)
    d = ((lab[:, None, :] - cent[None]) ** 2).sum(-1)
    labels = d.argmin(1).reshape(fg.shape)
    labels = ndi.median_filter(labels, 7)
    labels[~fg] = -1
    cent_rgb = cv2.cvtColor(cent.reshape(1, -1, 3).astype(np.uint8), cv2.COLOR_LAB2RGB).reshape(-1, 3) / 255
    return labels, cent_rgb.astype(np.float32), sm

# ---------------------------------------------------------------- ink
def _ink_skeleton(sm, fg, vig=None, pupil_frac=0.006):
    g = cv2.cvtColor(sm, cv2.COLOR_RGB2GRAY)
    g = cv2.GaussianBlur(g, (0, 0), 1.6)
    v = np.median(g[fg]) if fg.any() else 128
    e = cv2.Canny(g, int(max(12, 0.22 * v)), int(max(30, 0.55 * v)))
    e = (e > 0) & ndi.binary_dilation(fg, iterations=6)
    # silhouette always gets an outline
    cnt = cv2.morphologyEx(fg.astype(np.uint8), cv2.MORPH_GRADIENT, np.ones((3, 3), np.uint8)) > 0
    e = e | cnt
    if vig is not None:
        e &= ndi.binary_erosion(vig > 0.5, iterations=14)
    e[:6] = e[-6:] = False; e[:, :6] = e[:, -6:] = False
    e = ndi.binary_closing(e, iterations=2)
    sk = skeletonize(e)
    sk = remove_small_objects(sk, max_size=28, connectivity=2)
    dark = (cv2.cvtColor(sm, cv2.COLOR_RGB2LAB)[..., 0] < 50) & fg       # pupils, deep shadows -> solid ink
    dark = remove_small_objects(ndi.binary_opening(dark, iterations=2), max_size=60)
    lab, n = ndi.label(dark)                                   # only SMALL dark shapes become solid ink (eyes, buttons)
    if n:
        sizes = ndi.sum(dark, lab, range(1, n + 1))
        dark = np.isin(lab, 1 + np.nonzero(sizes < pupil_frac * W * W)[0])
    if vig is not None: dark &= vig > 0.6
    return sk, dark

def _flow(rng, amp, sigma=W / 14):
    return _noise((W, W), sigma, rng, amp), _noise((W, W), sigma, rng, amp)

def _warp(a, fx, fy, interp=cv2.INTER_LINEAR):
    gx, gy = np.meshgrid(np.arange(W, dtype=np.float32), np.arange(W, dtype=np.float32))
    return cv2.remap(a, gx + fx, gy + fy, interp, borderMode=cv2.BORDER_REPLICATE)

def _render_ink(sk, dark, rng, width=4.2):
    d = cv2.distanceTransform((~sk).astype(np.uint8), cv2.DIST_L2, 5)
    rad = width / 2 * (1 + _noise((W, W), 22, rng, 0.28) + _noise((W, W), 5, rng, 0.08))
    rad = np.clip(rad, 0.9, width)
    a = np.clip(rad - d + 0.5, 0, 1)
    if dark.any():
        dd = cv2.GaussianBlur(dark.astype(np.float32), (0, 0), 1.0)
        a = np.maximum(a, np.clip(dd * 1.6 - 0.3, 0, 1))
    # dry-brush breakup along strokes
    a *= np.clip(1 - np.clip(_noise((W, W), 1.2, rng, 0.35), 0, None) * (a < 0.98), 0.55, 1)
    return a.astype(np.float32)

# ---------------------------------------------------------------- washes
def _render_wash(labels, cent, rgb_src, fg, rng, grain, P):
    out_c = np.zeros((W, W, 3), np.float32); out_a = np.zeros((W, W), np.float32)
    lum = cv2.GaussianBlur(cv2.cvtColor((rgb_src * 255).astype(np.uint8), cv2.COLOR_RGB2GRAY).astype(np.float32) / 255, (0, 0), 6)
    for i, c in enumerate(cent):
        m = (labels == i).astype(np.float32)
        if m.sum() < 80: continue
        col = _harmonize(c, P)
        # soft wobbly edge + edge darkening (pigment pools at the rim)
        mb = cv2.GaussianBlur(m, (0, 0), 2.5)
        mb = np.clip((mb - 0.5 + _noise((W, W), 3, rng, 0.08)) * 4 + 0.5, 0, 1)
        inside = cv2.distanceTransform((mb > 0.5).astype(np.uint8), cv2.DIST_L2, 5)
        rim = np.exp(-inside / 7.0) * (mb > 0.5)
        a = mb * (0.78 + 0.22 * rim) * (0.92 + 0.5 * grain)
        out_c += col[None, None] * a[..., None]; out_a += a
    out_c = out_c / np.maximum(out_a, 1e-4)[..., None]
    # keep some of the original value structure (soft shading inside washes)
    shade = np.clip(0.80 + 0.35 * (lum - lum[fg].mean() if fg.any() else 0), 0.7, 1.08)
    out_c = np.clip(out_c * shade[..., None], 0, 1)
    out_a = np.clip(out_a, 0, 1)
    # a little bleed beyond the lines, and pigment blooms
    out_a = cv2.GaussianBlur(out_a, (0, 0), 1.2)
    return out_c, out_a

def pick_backdrop(cent, labels, seed, P):
    """Pastel palette wash behind the subject, chosen to contrast the subject's dominant hue."""
    hues = []
    if len(cent):
        cnt = np.array([(labels == i).sum() for i in range(len(cent))], np.float32)
        for i in np.argsort(-cnt)[:3]:
            h, l, s_ = colorsys.rgb_to_hls(*_harmonize(cent[i], P))
            if s_ > 0.15: hues.append(h)
    cands = []
    for p in PALETTE:
        ph, pl, ps = colorsys.rgb_to_hls(*_hex(p))
        d = min([min(abs(ph - h), 1 - abs(ph - h)) for h in hues] or [0.5])
        cands.append((d, p))
    lo, hi = P.backdrop_band
    band = [c for c in cands if lo <= c[0] <= hi] if hues else []
    if band:                                       # harmonious: split-complementary/triadic band, palette order = stable
        top = [c[1] for c in band]
    else:
        cands.sort(reverse=True)
        top = [c[1] for c in cands[:3]]
    p = top[seed % len(top)]
    h, l, s_ = colorsys.rgb_to_hls(*_hex(p))
    return np.array(colorsys.hls_to_rgb(h, 0.80, min(0.42, s_ + 0.05)), np.float32), p

def _render_backdrop(col, fg, rng, frng, base_blob):
    v = base_blob
    # leave a thin unpainted halo around the subject (painted-around look)
    halo = cv2.GaussianBlur(ndi.binary_dilation(fg, iterations=7).astype(np.float32), (0, 0), 2.0)
    a = v * (1 - halo)
    inside = cv2.distanceTransform((v > 0.5).astype(np.uint8), cv2.DIST_L2, 5)
    rim = np.exp(-inside / 10.0) * (v > 0.5)
    a = a * (0.62 + 0.30 * rim + 0.08 * _noise((W, W), 30, frng))
    return np.clip(a, 0, 1).astype(np.float32)

def _compose(paper_img, wash_c, wash_a, ink_a, back=None):
    p = paper_img
    if back is not None:
        bc, ba = back
        p = p * (1 - ba[..., None] * (1 - bc[None, None]))
    w = p * (1 - wash_a[..., None] * (1 - wash_c))           # multiply-like transparent wash
    ink = INK[None, None]
    return w * (1 - ink_a[..., None] * 0.94) + ink * ink_a[..., None] * 0.94

# ---------------------------------------------------------------- public
def stylize(im: Image.Image, kind="agent", seed=7, frames=BOIL_FRAMES, k=7, crop=0.90, preset=DEFAULT_PRESET):
    """Return (frames, meta). frames[0] is the still; all frames form a seamless boil loop at BOIL_FPS.
    kind: "agent" (object, circle-safe fit) or "group" (scenes that run off the canvas get an organic vignette).
    crop: fraction of the source kept (0.90 trims provider watermarks/borders; ~0.985 for clean sources)."""
    P = PRESETS[preset] if isinstance(preset, str) else preset
    rng = np.random.default_rng(seed)
    rgb = _prep(im, crop)
    fg, _ = _subject_mask(rgb)
    rgb, fg, vig = _recenter(rgb, fg, kind, rng, P)
    labels, cent, sm = _quantize(rgb, fg, k=k, seed=seed)
    sk, dark = _ink_skeleton(sm, fg, vig, P.pupil_frac)
    pap, grain = paper()
    out = []
    reg = rng.uniform(-3, 3, 2).astype(np.float32)               # hand-colouring misregistration
    use_back = P.backdrop and (vig is None or _border_touch(fg) <= 0.10)
    bcol, bhex = pick_backdrop(cent, labels, seed, P) if use_back else (None, None)
    bblob = _blob(np.random.default_rng(seed + 7), W * 0.415, wobble=0.045, soft=2.5) if use_back else None
    for f in range(frames):
        frng = np.random.default_rng(seed * 101 + f)
        fx, fy = _flow(frng, 1.6)                                 # line boil: per-frame redraw jitter
        sk_f = _warp(sk.astype(np.uint8) * 255, fx, fy) > 90
        sk_f = skeletonize(ndi.binary_dilation(sk_f, iterations=1))
        dark_f = _warp(dark.astype(np.uint8) * 255, fx, fy) > 127
        ink_a = _render_ink(sk_f, dark_f, frng)
        wx, wy = _flow(frng, 0.9, W / 10)
        lab_f = _warp(labels.astype(np.float32), wx + reg[0], wy + reg[1], cv2.INTER_NEAREST).astype(int)
        wc, wa = _render_wash(lab_f, cent, rgb, fg, frng, grain, P)
        if vig is not None:                                       # feathered, slightly blooming vignette edge
            v = _warp(vig, wx, wy)
            edge = 4 * v * (1 - v)                                # noise only where the vignette fades
            wa = wa * np.clip(v + 0.35 * edge * _noise((W, W), 5, frng), 0, 1)
            ink_a = ink_a * np.clip(v * 1.6 - 0.4, 0, 1)
        back = None
        if use_back:
            bb = _warp(bblob, wx * 1.5, wy * 1.5)
            back = (bcol, _render_backdrop(bcol, _warp(fg.astype(np.float32), fx, fy) > 0.5, rng, frng, bb))
        out.append(Image.fromarray((np.clip(_compose(pap, wc, wa, ink_a, back), 0, 1) * 255).astype(np.uint8)))
    meta = colors(labels, cent, P)
    if use_back:
        h, l, s_ = colorsys.rgb_to_hls(*_hex(bhex))
        meta["backdrop"] = bhex
        meta["tint"] = "#%02X%02X%02X" % tuple(int(round(x * 255)) for x in colorsys.hls_to_rgb(h, 0.50, min(0.5, s_)))
    meta.update(style=STYLE_VERSION if P.name == "v1" else f"{STYLE_VERSION}+{P.name}", preset=P.name, seed=seed, kind=kind, crop=crop)
    return out, meta

def colors(labels, cent, P=PRESETS[DEFAULT_PRESET]):
    """dominant = largest wash, accent = most saturated wash (>3% area), tint = deeper dominant for UI tinting."""
    if len(cent) == 0: return {"dominant": "#D8C3A0", "accent": "#7FAF6B", "tint": "#8C7859"}
    cnt = np.array([(labels == i).sum() for i in range(len(cent))])
    hcols = [_harmonize(c, P) for c in cent]
    dom = hcols[int(cnt.argmax())]
    sat = [colorsys.rgb_to_hls(*c)[2] * (cnt[i] > cnt.sum() * 0.03) for i, c in enumerate(hcols)]
    acc = hcols[int(np.argmax(sat))]
    # UI tint = deeper version of dominant, readable as a background tint
    h, l, s = colorsys.rgb_to_hls(*dom); tint = colorsys.hls_to_rgb(h, P.tint_l, min(P.tint_s_max, s + P.tint_s_add))
    tohex = lambda c: "#%02X%02X%02X" % tuple(int(round(x * 255)) for x in c)
    return {"dominant": tohex(dom), "accent": tohex(acc), "tint": tohex(tint)}

def dark_variant(im: Image.Image):
    """Dark-mode still: paper dimmed to a warm stone so the disc doesn't glare on black."""
    a = np.asarray(im).astype(np.float32) / 255
    a = a * np.array([0.86, 0.84, 0.80], np.float32)
    return Image.fromarray((np.clip(a, 0, 1) * 255).astype(np.uint8))

def export_asset(frames, meta, outdir, slug, anim_size=384, still_sizes=(1024, 512)):
    """Writes: <slug>.png (1024), <slug>@512.png, <slug>-dark@512.png, <slug>.webp (animated), <slug>.apng, <slug>.mp4"""
    d = pathlib.Path(outdir); d.mkdir(parents=True, exist_ok=True)
    files = {}
    for s in still_sizes:
        n = f"{slug}.png" if s == 1024 else f"{slug}@{s}.png"
        frames[0].resize((s, s), Image.LANCZOS).save(d / n, optimize=True); files[f"still{s}"] = n
    n = f"{slug}-dark@512.png"; dark_variant(frames[0].resize((512, 512), Image.LANCZOS)).save(d / n, optimize=True); files["stillDark512"] = n
    small = [f.resize((anim_size, anim_size), Image.LANCZOS) for f in frames]
    dur = int(1000 / BOIL_FPS)
    n = f"{slug}.webp"; small[0].save(d / n, save_all=True, append_images=small[1:], duration=dur, loop=0, quality=78, method=6); files["animWebP"] = n
    q = [f.quantize(colors=128, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE) for f in [x.resize((256, 256), Image.LANCZOS) for x in frames]]
    n = f"{slug}.apng"; q[0].save(d / n, save_all=True, append_images=q[1:], duration=dur, loop=0, format="PNG", optimize=True); files["animAPNG256"] = n
    with tempfile.TemporaryDirectory() as td:
        for i, f in enumerate(small): f.save(f"{td}/f{i}.png")
        n = f"{slug}.mp4"
        # 3 drawn frames, 4 cycles = 1.2 s seamless loop; player loops it
        subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-stream_loop", "3", "-framerate", str(BOIL_FPS), "-i", f"{td}/f%d.png",
                        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "24", "-preset", "slow", "-r", "30", "-movflags", "+faststart",
                        "-an", str(d / n)], check=True)
        files["animMP4"] = n
    return files
