#!/usr/bin/env bash
# Zoen's whole backend on this machine, one command:
#   scripts/dev-stack.sh            → private FoundationDB (scripts/fdb.sh, under .dev/fdb)
#                                     + private Postgres (downloaded once, under .dev/pg)
#                                     + zoen-relay on http://127.0.0.1:8787
#   DATABASE_URL=postgres://… scripts/dev-stack.sh   → use your own Postgres instead
#
# The iOS simulator and the Mac app reach it at 127.0.0.1:8787 (the default relay of a
# debug build); a phone on the same Wi-Fi uses `-RodaRelay http://<this-mac>.local:8787`.
# Try it from a terminal with the CLI: `target/debug/zoen --home /tmp/ana init --name Ana --handle ana`.
set -euo pipefail
cd "$(dirname "$0")/.."

DEV="${ZOEN_DEV_DIR:-.dev}"
BIND="${ZOEN_BIND:-0.0.0.0:8787}"
mkdir -p "$DEV/blobs"

echo "▸ FoundationDB"
scripts/fdb.sh up
eval "$(scripts/fdb.sh env)"

echo "▸ building zoen-relay and the zoen CLI"
cargo build -q -p zoen-relay --features embedded-pg -p zoen-cli

export ZOEN_BLOB_DIR="$DEV/blobs"
export ZOEN_RELAY_NAME="${ZOEN_RELAY_NAME:-zoen-dev}"
if [[ -n "${DATABASE_URL:-}" ]]; then
  echo "▸ relay on $BIND (Postgres: $DATABASE_URL)"
  exec target/debug/zoen-relay --bind "$BIND"
fi
echo "▸ relay on $BIND (private Postgres in $DEV/pg; first run downloads it)"
exec target/debug/zoen-relay --bind "$BIND" --embedded-pg "$DEV/pg"
