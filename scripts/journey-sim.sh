#!/usr/bin/env bash
# Milestone 1 on a real simulator: the relay, a person in the terminal (the zoen CLI, the
# same Rust core the app links) and the iPhone app driven by XCUITest
# (apple/UITests/RealSyncJourneyTests.swift). The screen is recorded; the video, the test's
# screenshots and Bruno's transcript land in OUT.
#   scripts/journey-sim.sh                 → starts the dev stack if nothing answers on :8787
#   SIM="iPhone 17" OUT=/tmp/m1 scripts/journey-sim.sh
# One simulator by default; extra ones only for multi-user runs, shut down right after.
set -euo pipefail
cd "$(dirname "$0")/.."

# One run at a time: two runs would fight over the same simulator.
mkdir -p .dev
exec 9>.dev/journey-sim.lock
if ! flock -n 9 2>/dev/null && ! shlock -f .dev/journey-sim.pid -p $$ 2>/dev/null; then
  echo "another journey-sim run is in progress"; exit 1
fi

SIM="${SIM:-iPhone 17 Pro}"
RELAY="${ZOEN_RELAY:-http://127.0.0.1:8787}"
RUN="$(date +%s | tail -c 6)"
ME="ana_$RUN"
PEER="bruno_$RUN"
MESSAGE="oi Bruno, é a Ana no simulador $RUN"
REPLY="oi Ana, aqui é o Bruno no terminal $RUN"
WORK="$(mktemp -d)"
OUT="${OUT:-$PWD/.dev/shots/real-m1}"
mkdir -p "$OUT"
echo "▸ run $RUN (work dir $WORK, artifacts in $OUT)"

LOCK="${ZOEN_SIM_LOCK:-/tmp/zoen-simulator.lock}"
for _ in $(seq 1 600); do mkdir "$LOCK" 2>/dev/null && break; sleep 1; done
[[ -d "$LOCK" && ! -f "$LOCK/owner" ]] || { echo "simulator lock $LOCK held by $(cat "$LOCK/owner" 2>/dev/null)"; exit 1; }
echo "journey-sim $RUN pid $$ $(date +%T)" > "$LOCK/owner"
release_lock() { rm -rf "$LOCK"; }
trap release_lock EXIT
if pgrep -q xcodebuild; then echo "another xcodebuild is running; the simulator is shared, try again when it's quiet"; exit 1; fi

BOOTED="$(xcrun simctl list devices booted | grep -c "(Booted)" || true)"
if (( BOOTED > 1 )); then echo "more than one simulator is booted; this journey needs one, shut the others down first"; exit 1; fi

scripts/fdb.sh up
eval "$(scripts/fdb.sh env)"
cargo build -q -p zoen-cli -p zoen-relay --features zoen-relay/embedded-pg
scripts/build-core.sh --if-stale
if ! curl -sf "$RELAY/healthz" >/dev/null; then
  echo "▸ starting the dev stack"
  scripts/dev-stack.sh > "$WORK/relay.log" 2>&1 &
  STACK=$!
  trap 'kill $STACK 2>/dev/null || true; release_lock' EXIT
  for _ in $(seq 1 180); do curl -sf "$RELAY/healthz" >/dev/null && break; sleep 1; done
  curl -sf "$RELAY/healthz" >/dev/null || { echo "relay didn't come up"; cat "$WORK/relay.log"; exit 1; }
fi

export ZOEN_RELAY="$RELAY"
zoen() { target/debug/zoen --home "$WORK/bruno" "$@"; }
zoen init --name Bruno --handle "$PEER" --relay "$RELAY"

echo "▸ building the app and the UI tests"
(cd apple && xcodegen generate --quiet)
xcodebuild build-for-testing -project apple/Zoen.xcodeproj -scheme ZoenUITests \
  -destination "platform=iOS Simulator,name=$SIM" -derivedDataPath build/DerivedData -quiet

# Bruno answers once Ana's message reaches him; the clock starts after the build.
(
  for _ in $(seq 1 300); do
    if zoen read "@$ME" 2>/dev/null | grep -qF "$MESSAGE"; then
      zoen send "@$ME" "$REPLY" >/dev/null
      echo "▸ Bruno got it and answered"
      exit 0
    fi
    sleep 1
  done
  echo "▸ Bruno never got the message"
) &
PEER_PID=$!

echo "▸ app on $SIM as @$ME, talking to @$PEER"
xcrun simctl boot "$SIM" 2>/dev/null || true
xcrun simctl bootstatus "$SIM" -b >/dev/null
xcrun simctl io "$SIM" recordVideo --codec h264 --force "$OUT/journey-$RUN.mp4" > "$WORK/rec.log" 2>&1 &
REC=$!
STATUS=0
TEST_RUNNER_ZOEN_ME="$ME" TEST_RUNNER_ZOEN_PEER="$PEER" TEST_RUNNER_ZOEN_RELAY="$RELAY" \
TEST_RUNNER_ZOEN_MESSAGE="$MESSAGE" TEST_RUNNER_ZOEN_REPLY="$REPLY" \
xcodebuild test-without-building -project apple/Zoen.xcodeproj -scheme ZoenUITests \
  -destination "platform=iOS Simulator,name=$SIM" -derivedDataPath build/DerivedData \
  -only-testing:ZoenUITests/RealSyncJourneyTests -quiet \
  -collect-test-diagnostics never -resultBundlePath "$WORK/journey.xcresult" || STATUS=$?
kill -INT "$REC" 2>/dev/null || true
wait "$REC" 2>/dev/null || true
pkill -P "$PEER_PID" 2>/dev/null || true
kill "$PEER_PID" 2>/dev/null || true
wait "$PEER_PID" 2>/dev/null || true

rm -rf "$WORK/att" && mkdir -p "$WORK/att"
xcrun xcresulttool export attachments --path "$WORK/journey.xcresult" --output-path "$WORK/att" >/dev/null 2>&1 || true
python3 - "$WORK/att" "$OUT" "$RUN" <<'PY'
import json, os, shutil, sys
src, out, run = sys.argv[1:]
try:
    manifest = json.load(open(os.path.join(src, "manifest.json")))
except Exception:
    sys.exit(0)
for test in manifest:
    for a in test.get("attachments", []):
        name = a.get("suggestedHumanReadableName", "")
        if name.endswith(".png"):
            label = name.split("_0_")[0].replace(": ", "-").replace(" ", "-")
            shutil.copy(os.path.join(src, a["exportedFileName"]), os.path.join(out, f"{run}-{label}.png"))
PY
cp -R "$WORK/journey.xcresult" "$OUT/journey-$RUN.xcresult" 2>/dev/null || true

echo "▸ Bruno's side:"
zoen read "@$ME" | tee "$OUT/bruno-terminal-$RUN.txt"
zoen verify | tee -a "$OUT/bruno-terminal-$RUN.txt"
echo "▸ $([[ $STATUS == 0 ]] && echo "journey passed" || echo "journey failed ($STATUS)"); artifacts in $OUT"
exit "$STATUS"
