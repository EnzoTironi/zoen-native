"""System One-compatible server for laya-mlx (Apple Silicon, in-process MLX).

    HF_HOME=.tools/hf .tools/venv-laya/bin/python tools/gate-serve/laya_serve.py \
        --model aac6fef/laya-typed-decisions-mlx --port 8010

POST /v1/systemone  {"state": str, "questions": {...}, "model": str}
  -> {"model", "answers", "usage", "latency_ms"}   (same shape as TypeSafe's API / Kev)
GET  /health

Binds to 127.0.0.1 only; no auth (local dev). Stdlib HTTP, no extra deps.
"""
import argparse, json, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Lock

import laya_mlx

ap = argparse.ArgumentParser()
ap.add_argument("--model", default="aac6fef/laya-typed-decisions-mlx")
ap.add_argument("--port", type=int, default=8010)
ap.add_argument("--cache-limit-mb", type=int, default=256,
                help="MLX keeps freed GPU buffers around; without a cap the server grew to ~7 GB on a 16 GB Mac")
args = ap.parse_args()

import mlx.core as mx
_set = getattr(mx, "set_cache_limit", None) or mx.metal.set_cache_limit
_set(args.cache_limit_mb * 1024 * 1024)

t0 = time.time()
agent = laya_mlx.load(args.model)
print(f"loaded {args.model} in {time.time() - t0:.1f}s", flush=True)
lock = Lock()  # MLX graph is not shared across threads


class H(BaseHTTPRequestHandler):
    def _send(self, code, obj):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        self._send(200, {"ok": True, "model": args.model}) if self.path == "/health" else self._send(404, {"error": "not found"})

    def do_POST(self):
        if self.path.rstrip("/") not in ("/v1/systemone", "/v1/system_one", "/v1/system-one"):
            return self._send(404, {"error": "not found"})
        try:
            req = json.loads(self.rfile.read(int(self.headers.get("content-length", 0))))
            start = time.time()
            with lock:
                out = agent.system_one(req["state"], req["questions"])
                mx.clear_cache() if hasattr(mx, "clear_cache") else mx.metal.clear_cache()
            answers = out.get("answers", out) if isinstance(out, dict) else out
            self._send(200, {"model": req.get("model") or args.model, "answers": answers,
                             "latency_ms": round((time.time() - start) * 1000)})
        except Exception as e:  # report, don't crash the server
            self._send(400, {"error": f"{type(e).__name__}: {e}"})

    def log_message(self, *a):
        pass


print(f"serving System One on http://127.0.0.1:{args.port}/v1/systemone", flush=True)
ThreadingHTTPServer(("127.0.0.1", args.port), H).serve_forever()
