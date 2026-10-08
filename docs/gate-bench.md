# Safety gate: local decision models on the Mac

Measured 2026-10-08 on Enzo's MacBook Pro (Apple M5, **16 GB** RAM, not 32; about 18 GB of swap
already in use from Xcode, the simulator and OrbStack), with `crates/roda-gate/examples/gate_bench.rs`
running the 21 red-team fixtures (`crates/roda-gate/fixtures/redteam.json`: 15 unsafe, 6 benign)
through `SystemOneHttp` → `http://127.0.0.1:<port>/v1/systemone`.

"Model only" turns the hard rules off. "Rules+model" is the real gate. Precision and recall treat
unsafe as positive (flagged means ask or block). "Exact" means the outcome (allow/ask/block)
matched the label. AUROC ranks one risk number per case (the most alarming answer) and only
counts cases the model decided.

| Model (MLX, served locally) | Mode | Precision | Recall | Exact | AUROC | p50 / p95 | Memory (footprint, peak) |
|---|---|---|---|---|---|---|---|
| **Laya English** `aac6fef/laya-mlx` (421M) | model only, default thresholds | 0.75 | 1.00 | 11/21 | **1.00** | 83 / 339 ms | 0.9 GB, 1.6 GB peak |
| | rules+model, `laya_local` preset | **1.00** | **1.00** | 17/21 | 1.00 | 82 / 125 ms | |
| Laya typed-decisions `aac6fef/laya-typed-decisions-mlx` | model only, default | 0.75 | 1.00 | 6/21 | 0.89 | 71 / 223 ms | 0.9 GB, 1.6 GB peak |
| | rules+model, `laya_local` | 0.94 | 1.00 | 14/21 | 0.96 | 90 / 124 ms | |
| Kev-0.8B `jaredpalmer/kev-0.8b@v1.0` (Qwen3.5-0.8B + LoRA, bf16) | model only, default | 0.78 | 0.93 | 6/21 | 0.72 | 97 / 136 ms | 3.1 GB |
| | rules+model, default | 0.79 | 1.00 | 11/21 | 0.81 | 92 / 107 ms | |
| Kev-4B / Kev-9B | not run | – | – | – | – | – | needs a 32 GB Mac (8.4 GB of bf16 weights, about 10–12 GB footprint per its card) and about 9 GB of disk (13 GB was free with a 10 GB floor). Kev's MLX loader merges the LoRA into bf16 base weights, so a 4-bit base isn't an option. |
| Mock (keyword heuristics, written alongside the fixtures) | rules+model | 1.00 | 1.00 | 18/21 | – | <1 ms | – |

**Winner: Laya English (`aac6fef/laya-mlx`).** It ranked best (perfect separation on this small
set) and it's the lightest and fastest. With the default thresholds every local model just says
"ask" for nearly everything: their score confidences sit under 0.35, so "low confidence" means
ask. The fitted `Thresholds::laya_local()` fixes that, **but it was fitted on these same 21
fixtures**, so treat 1.00/1.00 as an upper bound until we re-fit on a held-out set. One margin is
razor thin: the mass email scores 0.48 risk and the benign vote 0.46. Laya's `harmful_content`
score isn't usable either (it gave a plain itinerary 0.80 of 3), so the preset ignores it.

Latency is noisy on this Mac because of memory pressure. Before the MLX buffer cache was capped,
the first Laya run grew to a **7 GB** footprint and hit seconds per call. `laya_serve.py` now
caps it at 256 MB and clears it after each request.

Jev (TypeSafe API) stays the production target with the default thresholds. Benchmarking it
or OpenAI Decisions needs real keys (`GATE_BEARER_<NAME>` in a dev .env, never in the repo).

Disk on the Mac (everything under `~/Developer/roda/.tools`, gitignored): 1.1 GB kept
(Laya model 808 MB, venv 264 MB, Python 3.13 72 MB). Kev-0.8B (about 2.7 GB with its venv) and
Laya typed (807 MB) were removed after benchmarking. Free disk: 13 GB before, 14 GB after
(`target/debug` and the uv cache were also cleaned).

Run it: `tools/gate-serve/run.sh` (port 8011), then
`GATE_PROVIDERS="laya=http://127.0.0.1:8011#laya" cargo run -p roda-gate --example gate_bench`.

## Held-out set (2026-10-08)

`crates/roda-gate/fixtures/heldout.json`: 20 new cases (10 unsafe, 10 safe), written after
`Thresholds::laya_local()` was calibrated and never used to tune it. Run with
`GATE_FIXTURES=heldout GATE_PROVIDERS="laya=http://127.0.0.1:8011#laya" cargo run -p roda-gate --example gate_bench`.

| provider · mode | precision | recall | exact | tp/fp/fn/tn | AUROC | p50 | p95 |
|---|---|---|---|---|---|---|---|
| mock · rules+model | 0.88 | 0.70 | 15/20 | 7/1/3/9 | 0.70 | 0 ms | 0 ms |
| laya (English) · rules+model · default | 0.53 | 0.90 | 6/20 | 9/8/1/2 | 0.85 | 60 ms | 81 ms |
| laya (English) · rules+model · laya_local | 0.67 | 0.80 | 10/20 | 8/4/2/6 | 0.85 | 64 ms | 101 ms |

Held-out recall/precision with the preset (0.80 / 0.67) is below the calibration set, as
expected. Misses include the tracker bundle (allowed by the model); most exfiltration cases
land on "ask" rather than "block". False asks are on benign tool calls (vote, itinerary,
rename). Laya stays dev-only; Jev remains the production target.
