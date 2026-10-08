#!/usr/bin/env bash
# Two real clients (the zoen CLI, the same core as the app) against a relay that is already
# running somewhere: the local k3d cluster, Fly staging, anything.
#   scripts/journey-remote.sh https://relay.tryzoen.com
# Proves: accounts, handle lookup, a DM both ways, an encrypted photo through the blob store,
# every process restarted between steps (each command is a fresh process), chains verified.
set -euo pipefail
cd "$(dirname "$0")/.."
RELAY="${1:?relay url}"
RUN="$(date +%s)$RANDOM"; RUN="${RUN: -6}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cargo build -q -p zoen-cli
ana() { target/debug/zoen --home "$WORK/ana" "$@"; }
bruno() { target/debug/zoen --home "$WORK/bruno" "$@"; }
step() { echo "▸ $*"; }

curl -sf -m 10 "${RELAY%/}/healthz" >/dev/null || { echo "relay not healthy at $RELAY"; exit 1; }
step "relay healthy at $RELAY"
ana init --name Ana --handle "ana_$RUN" --relay "$RELAY" >/dev/null
bruno init --name Bruno --handle "bruno_$RUN" --relay "$RELAY" >/dev/null
step "accounts @ana_$RUN and @bruno_$RUN registered"
ana people "bruno_$RUN" | grep -q "@bruno_$RUN"
step "Ana finds Bruno by handle"

MSG="oi Bruno $RUN"; REPLY="oi Ana $RUN"
ana dm "@bruno_$RUN" "$MSG" >/dev/null
for _ in $(seq 1 30); do bruno read "@ana_$RUN" 2>/dev/null | grep -qF "$MSG" && break; sleep 1; done
bruno read "@ana_$RUN" | grep -qF "$MSG"
step "Bruno received: $MSG"
bruno send "@ana_$RUN" "$REPLY" >/dev/null
for _ in $(seq 1 30); do ana read "@bruno_$RUN" 2>/dev/null | grep -qF "$REPLY" && break; sleep 1; done
ana read "@bruno_$RUN" | grep -qF "$REPLY"
step "Ana received: $REPLY"

head -c 200000 /dev/urandom > "$WORK/photo.jpg"
ana background "@bruno_$RUN" --photo "$WORK/photo.jpg" >/dev/null
bruno photo "@ana_$RUN" --out "$WORK/got.jpg" >/dev/null
cmp -s "$WORK/photo.jpg" "$WORK/got.jpg"
step "encrypted photo crossed the blob store byte for byte"

ana verify >/dev/null && bruno verify >/dev/null
step "both devices verify every signed chain"
echo "journey passed against $RELAY"
