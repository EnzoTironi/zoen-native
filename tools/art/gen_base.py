"""Provider adapter: fetch base art from a free text-to-image endpoint (Pollinations anonymous tier, model 'sana').
The provider is only a shape/colour source; the Zoen look (ink, washes, paper, palette, boil) is applied by zoen_style.py,
so swapping providers keeps the set consistent."""
import sys, time, urllib.parse, urllib.request, pathlib, json

STYLE = ("cute simple illustration with bright cheerful colors, black ink pen outlines with soft watercolor wash, children's picture book, "
         "isolated on plain white background, centered with wide margins, whole subject visible")
NEG = "dark background, black background, circle background, grayscale, monochrome, frame, border, photo, photorealistic, 3d render, text, letters, watermark, cropped"

def fetch(prompt, out, seed=1, size=1024, tries=12):
    q = {"width": size, "height": size, "seed": seed, "negative_prompt": NEG}
    url = "https://image.pollinations.ai/prompt/" + urllib.parse.quote(prompt) + "?" + urllib.parse.urlencode(q)
    for i in range(tries):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "zoen-art/0.1"})
            with urllib.request.urlopen(req, timeout=180) as r:
                data = r.read()
                if r.headers.get("content-type", "").startswith("image") and len(data) > 10000:
                    pathlib.Path(out).write_bytes(data); return True
        except Exception as e:
            print("  retry", i, getattr(e, "code", e), file=sys.stderr, flush=True)
        time.sleep(15)
    return False

if __name__ == "__main__":
    # usage: gen_base.py jobs.json outdir [ids...]   (ids => force regenerate those; ids may be id:seed)
    jobs = json.load(open(sys.argv[1])); outdir = pathlib.Path(sys.argv[2]); outdir.mkdir(parents=True, exist_ok=True)
    import os
    if os.environ.get("REV"): jobs = jobs[::-1]
    only = dict((a.split(":") + [None])[:2] for a in sys.argv[3:])
    for j in jobs:
        if only and j["id"] not in only: continue
        seed = int(only[j["id"]]) if only.get(j["id"]) else j.get("seed", 11)
        out = outdir / f'{j["id"]}.jpg'
        if out.exists() and not only: continue
        ok = fetch(j["subject"] + ", " + STYLE, out, seed=seed)
        print(j["id"], seed, "ok" if ok else "FAIL", time.strftime("%T"), flush=True)
        time.sleep(8)
