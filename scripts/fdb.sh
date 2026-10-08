#!/usr/bin/env bash
# A private FoundationDB 7.3 for development: binaries under .tools/fdb, data under .dev/fdb,
# no root needed. Idempotent: running it again reuses the install and the database.
#   scripts/fdb.sh up      install if needed, start fdbserver on 127.0.0.1:4500, configure once
#   scripts/fdb.sh env     print the exports builds and tests need (eval "$(scripts/fdb.sh env)")
#   scripts/fdb.sh down    stop the server
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"
VERSION="${FDB_VERSION:-7.3.71}"
TOOLS="$ROOT/.tools/fdb"
DEV="$ROOT/.dev/fdb"
PORT="${FDB_PORT:-4500}"
# Ratekeeper stops admitting transactions when free disk drops under 5% of the volume: 23 GB on
# a 460 GB laptop, so a dev box with 10 GB free hangs every write. Dev only: 1% (and FDB's own
# 100 MB floor still applies). Production keeps the defaults (infra/k8s, infra/fly).
MIN_FREE_RATIO="${FDB_MIN_FREE_RATIO:-0.01}"
CLUSTER="$DEV/fdb.cluster"

install() {
  [[ -x "$TOOLS/bin/fdbserver" ]] && return
  mkdir -p "$TOOLS/bin" "$TOOLS/lib" "$TOOLS/include"
  local tmp; tmp="$(mktemp -d)"
  local base="https://github.com/apple/foundationdb/releases/download/$VERSION"
  case "$(uname -s)" in
    Linux)
      for p in clients server; do
        curl -fsSL -o "$tmp/$p.deb" "$base/foundationdb-${p}_${VERSION}-1_amd64.deb"
        dpkg -x "$tmp/$p.deb" "$tmp/root"
      done
      cp "$tmp/root/usr/sbin/fdbserver" "$tmp/root/usr/bin/fdbcli" "$TOOLS/bin/"
      cp "$tmp/root/usr/lib/libfdb_c.so" "$TOOLS/lib/"
      cp -R "$tmp/root/usr/include/foundationdb" "$TOOLS/include/"
      ;;
    Darwin)
      local arch; arch="$(uname -m)"; [[ "$arch" == "arm64" ]] || arch="x86_64"
      curl -fsSL -o "$tmp/fdb.pkg" "$base/FoundationDB-${VERSION}_${arch}.pkg"
      pkgutil --expand-full "$tmp/fdb.pkg" "$tmp/x"
      cp "$(find "$tmp/x" -name fdbserver -type f | head -1)" "$(find "$tmp/x" -name fdbcli -type f | head -1)" "$TOOLS/bin/"
      cp "$(find "$tmp/x" -name 'libfdb_c.dylib' -type f | head -1)" "$TOOLS/lib/"
      cp -R "$(find "$tmp/x" -path '*include/foundationdb' -type d | head -1)" "$TOOLS/include/"
      ;;
    *) echo "unsupported OS"; exit 1 ;;
  esac
  rm -rf "$tmp"
}

env_exports() {
  echo "export FDB_CLUSTER_FILE=\"$CLUSTER\""
  echo "export LIBRARY_PATH=\"$TOOLS/lib\${LIBRARY_PATH:+:\$LIBRARY_PATH}\""
  echo "export LD_LIBRARY_PATH=\"$TOOLS/lib\${LD_LIBRARY_PATH:+:\$LD_LIBRARY_PATH}\""
  echo "export DYLD_LIBRARY_PATH=\"$TOOLS/lib\${DYLD_LIBRARY_PATH:+:\$DYLD_LIBRARY_PATH}\""
}

up() {
  install
  mkdir -p "$DEV/data" "$DEV/logs"
  [[ -f "$CLUSTER" ]] || echo "zoen:dev@127.0.0.1:$PORT" > "$CLUSTER"
  if ! pgrep -f "fdbserver.*$DEV/data" >/dev/null; then
    nohup "$TOOLS/bin/fdbserver" -p "127.0.0.1:$PORT" -C "$CLUSTER" -d "$DEV/data" -L "$DEV/logs" \
      --knob_min_available_space_ratio="$MIN_FREE_RATIO" \
      --knob_min_available_space_ratio_safety_buffer=0 \
      > "$DEV/fdbserver.out" 2>&1 &
    echo $! > "$DEV/fdbserver.pid"
  fi
  local fdbcli=("$TOOLS/bin/fdbcli" -C "$CLUSTER" --timeout 10)
  # A brand-new cluster answers nothing until it is configured, so configure exactly once.
  if [[ ! -f "$DEV/.configured" ]]; then
    "${fdbcli[@]}" --exec "configure new single ssd" >/dev/null 2>&1 || true
    touch "$DEV/.configured"
  fi
  for _ in $(seq 1 60); do
    if "${fdbcli[@]}" --exec "status minimal" 2>/dev/null | grep -q "is available"; then
      echo "fdb up ($CLUSTER)"; return
    fi
    sleep 1
  done
  echo "fdb didn't become available; see $DEV/logs"; exit 1
}

down() {
  [[ -f "$DEV/fdbserver.pid" ]] && kill "$(cat "$DEV/fdbserver.pid")" 2>/dev/null || true
  rm -f "$DEV/fdbserver.pid"
}

case "${1:-up}" in
  up) up ;;
  env) env_exports ;;
  down) down ;;
  *) echo "usage: scripts/fdb.sh [up|env|down]"; exit 2 ;;
esac
