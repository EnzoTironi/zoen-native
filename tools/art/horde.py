"""Provider adapter #2: AI Horde (crowdsourced, free, anonymous key). Async: submit -> poll -> download."""
import json, sys, time, urllib.request, pathlib, os
API = "https://aihorde.net/api/v2"
KEY = os.environ.get("HORDE_KEY", "0000000000")
HDR = {"User-Agent": "curl/8.5.0", "apikey": KEY, "Content-Type": "application/json", "Client-Agent": "zoen-art:0.1:assets"}

def _req(method, path, body=None):
    r = urllib.request.Request(API + path, data=json.dumps(body).encode() if body else None, headers=HDR, method=method)
    with urllib.request.urlopen(r, timeout=60) as resp: return json.loads(resp.read())

def submit(prompt, seed, model="Flux.1-Schnell fp8 (Compact)", size=512, steps=4, cfg=1.0, neg=None):
    p = prompt + (" ### " + neg if neg else "")
    body = {"prompt": p, "params": {"width": size, "height": size, "steps": steps, "cfg_scale": cfg, "seed": str(seed),
            "sampler_name": "k_euler", "n": 1}, "models": [model], "nsfw": False, "censor_nsfw": True,
            "r2": True, "shared": False, "slow_workers": True, "trusted_workers": False}
    return _req("POST", "/generate/async", body)["id"]

def wait(jid, out, timeout=1500):
    t0 = time.time()
    while time.time() - t0 < timeout:
        try: s = _req("GET", f"/generate/check/{jid}")
        except Exception as e: print("check err", e, file=sys.stderr); time.sleep(10); continue
        if s.get("faulted"): return False
        if s.get("done"):
            g = _req("GET", f"/generate/status/{jid}")["generations"]
            if not g or g[0].get("censored"): return False
            with urllib.request.urlopen(g[0]["img"], timeout=120) as r: pathlib.Path(out).write_bytes(r.read())
            return True
        time.sleep(8)
    return False

if __name__ == "__main__":
    jid = submit(sys.argv[1], int(sys.argv[3]) if len(sys.argv) > 3 else 11)
    print("job", jid, flush=True)
    print(wait(jid, sys.argv[2]))
