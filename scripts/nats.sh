#!/usr/bin/env bash
# A private NATS server with JetStream for development: binary under .tools/nats, state under
# .dev/nats, no root needed. Idempotent.
#   scripts/nats.sh up      install if needed (checksum-verified), start on 127.0.0.1:4222
#   scripts/nats.sh env     print the exports relays and tests need (eval "$(scripts/nats.sh env)")
#   scripts/nats.sh down    stop the server
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"
VERSION="${NATS_VERSION:-v2.15.0}"
TOOLS="$ROOT/.tools/nats"
DEV="$ROOT/.dev/nats"
PORT="${NATS_PORT:-4222}"

install() {
  [[ -x "$TOOLS/nats-server" ]] && "$TOOLS/nats-server" --version | grep -qF "${VERSION#v}" && return
  mkdir -p "$TOOLS"
  local tmp; tmp="$(mktemp -d)"
  local os arch
  case "$(uname -s)" in Linux) os=linux ;; Darwin) os=darwin ;; *) echo "unsupported OS"; exit 1 ;; esac
  case "$(uname -m)" in x86_64|amd64) arch=amd64 ;; arm64|aarch64) arch=arm64 ;; *) echo "unsupported arch"; exit 1 ;; esac
  local name="nats-server-$VERSION-$os-$arch"
  local base="https://github.com/nats-io/nats-server/releases/download/$VERSION"
  curl -fsSL -o "$tmp/$name.tar.gz" "$base/$name.tar.gz"
  curl -fsSL -o "$tmp/SHA256SUMS" "$base/SHA256SUMS"
  (cd "$tmp" && grep " $name.tar.gz\$" SHA256SUMS | shasum -a 256 -c - >/dev/null) \
    || { echo "nats-server checksum mismatch"; exit 1; }
  tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
  cp "$tmp/$name/nats-server" "$TOOLS/"
  rm -rf "$tmp"
}

env_exports() {
  echo "export ZOEN_NATS_URL=\"nats://127.0.0.1:$PORT\""
}

up() {
  install
  mkdir -p "$DEV/js"
  if ! pgrep -f "nats-server.*$DEV/js" >/dev/null; then
    nohup "$TOOLS/nats-server" -a 127.0.0.1 -p "$PORT" -js -sd "$DEV/js" \
      > "$DEV/nats.out" 2>&1 &
    echo $! > "$DEV/nats.pid"
  fi
  for _ in $(seq 1 50); do
    if (exec 3<>"/dev/tcp/127.0.0.1/$PORT") 2>/dev/null; then echo "nats up (127.0.0.1:$PORT)"; return; fi
    sleep 0.1
  done
  echo "nats didn't come up; see $DEV/nats.out"; exit 1
}

down() {
  [[ -f "$DEV/nats.pid" ]] && kill "$(cat "$DEV/nats.pid")" 2>/dev/null || true
  rm -f "$DEV/nats.pid"
}

case "${1:-up}" in
  up) up ;;
  env) env_exports ;;
  down) down ;;
  *) echo "usage: scripts/nats.sh [up|env|down]"; exit 2 ;;
esac
