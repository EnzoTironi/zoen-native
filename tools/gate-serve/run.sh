#!/bin/bash
# Local safety-gate model for dev: Laya (English, MLX) behind a System One endpoint.
# Everything stays inside the repo's .tools/ (venv, Python, HF cache). Apple Silicon only.
#   tools/gate-serve/run.sh            # http://127.0.0.1:8011/v1/systemone
set -e
cd "$(dirname "$0")/../.."
T=$PWD/.tools
export HF_HOME=$T/hf HF_HUB_DISABLE_TELEMETRY=1 UV_CACHE_DIR=$T/uv-cache UV_PYTHON_INSTALL_DIR=$T/python
if [ ! -x $T/venv-laya/bin/python ]; then
  uv venv -q --python 3.13 $T/venv-laya
  uv pip install -q --python $T/venv-laya/bin/python laya-mlx
  rm -rf $T/uv-cache
fi
[ -f $T/gate.env ] || cat > $T/gate.env <<ENV
# dev only (gitignored). The app/tests read these for SystemOneHttp.
GATE_BASE_URL=http://127.0.0.1:${PORT:-8011}
GATE_MODEL=laya
GATE_THRESHOLDS=laya_local
# Production target (not set here): GATE_BASE_URL=https://api.typesafe.ai, model jev-latest, bearer from the secret store.
ENV
exec $T/venv-laya/bin/python tools/gate-serve/laya_serve.py --model "${MODEL:-aac6fef/laya-mlx}" --port "${PORT:-8011}"
