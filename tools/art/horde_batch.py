import json, sys, time, pathlib, urllib.request
sys.path.insert(0, str(pathlib.Path(__file__).parent))
import horde
STYLE = ("loose black ink pen line drawing with soft watercolor washes, children's picture book illustration, cheerful colors, "
         "isolated on plain white paper background, centered, whole subject visible, no text, no signature")
jobs = json.load(open(pathlib.Path(__file__).parent / "jobs.json"))
out = pathlib.Path(sys.argv[1]); out.mkdir(parents=True, exist_ok=True)
state_f = out / "_horde_state.json"
state = json.load(open(state_f)) if state_f.exists() else {}
only = set(sys.argv[2:])
def body(j):
    return {"prompt": j["subject"] + ", " + STYLE, "params": {"width": 512, "height": 512, "steps": 4, "cfg_scale": 1, "n": 1,
            "sampler_name": "k_euler", "seed": str(j.get("hseed", 11))}, "models": ["Flux.1-Schnell fp8 (Compact)"], "nsfw": False}
for j in jobs:
    if only and j["id"] not in only: continue
    if (out / f'{j["id"]}.webp').exists() and not only: continue
    if j["id"] in state and not only: continue
    for t in range(3):
        try:
            state[j["id"]] = horde._req("POST", "/generate/async", body(j))["id"]; break
        except Exception as e:
            print("submit err", j["id"], e, getattr(e, "read", lambda: b"")()[:200], flush=True); time.sleep(5)
    json.dump(state, open(state_f, "w")); time.sleep(1.5)
print("submitted", len(state), flush=True)
pending = dict(state)
while pending:
    for sid, jid in list(pending.items()):
        try: s = horde._req("GET", f"/generate/check/{jid}")
        except Exception as e:
            if "404" in str(e): print(sid, "expired"); pending.pop(sid); state.pop(sid, None)
            continue
        if s.get("faulted"): print(sid, "faulted", flush=True); pending.pop(sid); state.pop(sid, None); continue
        if s.get("done"):
            g = horde._req("GET", f"/generate/status/{jid}")["generations"]
            if g and not g[0].get("censored"):
                with urllib.request.urlopen(g[0]["img"], timeout=120) as r: (out / f"{sid}.webp").write_bytes(r.read())
                print(sid, "ok", time.strftime("%T"), flush=True)
            else: print(sid, "censored/empty", flush=True)
            pending.pop(sid); state.pop(sid, None)
        time.sleep(0.7)
    json.dump(state, open(state_f, "w"))
    time.sleep(20)
print("all done")
