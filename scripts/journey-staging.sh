#!/usr/bin/env bash
# Release app on the simulator against relay.tryzoen.com, the other person played by the zoen
# CLI next to it. Recorded like journey-sim. The core ships arm64 simulator slices only, so the
# Release build is pinned to arm64.
#   scripts/journey-staging.sh
set -euo pipefail
cd "$(dirname "$0")/.."
SIM="${SIM:-iPhone 17 Pro}"
RELAY="${ZOEN_RELAY:-https://relay.tryzoen.com}"
RUN="$(date +%s | tail -c 6)"
ME="ana_s$RUN"; PEER="bruno_s$RUN"
MESSAGE="oi Ana, Bruno aqui pelo relay.tryzoen.com $RUN"
REPLY="oi Bruno, recebi no app Release $RUN"
OUT="${OUT:-$PWD/.dev/shots/staging}"; mkdir -p "$OUT"
WORK="$(mktemp -d)"
BOOTED="$(xcrun simctl list devices booted | grep -c "(Booted)" || true)"
if (( BOOTED > 1 )); then echo "more than one simulator is booted; shut the others down first"; exit 1; fi
xcrun simctl boot "$SIM" 2>/dev/null || true
xcrun simctl bootstatus "$SIM" -b >/dev/null

cargo build -q -p zoen-cli
scripts/build-core.sh --if-stale
zoen() { target/debug/zoen --home "$WORK/bruno" "$@"; }
zoen init --name Bruno --handle "$PEER" --relay "$RELAY" >/dev/null

(cd apple && xcodegen generate --quiet)
xcodebuild build-for-testing -project apple/Zoen.xcodeproj -scheme ZoenUITests -configuration Release \
  -destination "platform=iOS Simulator,name=$SIM" -derivedDataPath build/DerivedData -quiet ARCHS=arm64 ONLY_ACTIVE_ARCH=YES
xcrun simctl uninstall "$SIM" xyz.tironi.zoen 2>/dev/null || true

(
  for _ in $(seq 1 300); do zoen people "$ME" 2>/dev/null | grep -q "@$ME" && break; sleep 1; done
  zoen dm "@$ME" "$MESSAGE" >/dev/null && echo "▸ Bruno wrote to @$ME"
  for _ in $(seq 1 300); do zoen read "@$ME" 2>/dev/null | grep -qF "$REPLY" && { echo "▸ Bruno got the answer"; exit 0; }; sleep 1; done
  echo "▸ Bruno never got the answer"; exit 1
) &
PEER_PID=$!

xcrun simctl io "$SIM" recordVideo --codec h264 --force "$OUT/staging-$RUN.mp4" > "$WORK/rec.log" 2>&1 &
REC=$!
STATUS=0
TEST_RUNNER_ZOEN_ME="$ME" TEST_RUNNER_ZOEN_MESSAGE="$MESSAGE" TEST_RUNNER_ZOEN_REPLY="$REPLY" \
xcodebuild test-without-building -project apple/Zoen.xcodeproj -scheme ZoenUITests -configuration Release \
  -destination "platform=iOS Simulator,name=$SIM" -derivedDataPath build/DerivedData \
  -only-testing:ZoenUITests/StagingJourneyTests -quiet \
  -collect-test-diagnostics never -resultBundlePath "$WORK/staging.xcresult" ARCHS=arm64 ONLY_ACTIVE_ARCH=YES || STATUS=$?
kill -INT "$REC" 2>/dev/null || true; wait "$REC" 2>/dev/null || true
PEER_STATUS=0; wait "$PEER_PID" || PEER_STATUS=$?

mkdir -p "$WORK/att"
xcrun xcresulttool export attachments --path "$WORK/staging.xcresult" --output-path "$WORK/att" >/dev/null 2>&1 || true
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
cp -R "$WORK/staging.xcresult" "$OUT/staging-$RUN.xcresult" 2>/dev/null || true
zoen read "@$ME" | tee "$OUT/bruno-terminal-$RUN.txt"
echo "▸ app: $([[ $STATUS == 0 ]] && echo passed || echo "failed ($STATUS)"), Bruno: $([[ $PEER_STATUS == 0 ]] && echo passed || echo failed); artifacts in $OUT"
[[ $STATUS == 0 && $PEER_STATUS == 0 ]]
