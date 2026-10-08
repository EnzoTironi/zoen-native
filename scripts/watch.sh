#!/usr/bin/env bash
# Live watch mode for Zoen.
#
# Watches apple/ and crates/; on every change (debounced) it rebuilds only what changed:
# crates/ → Rust core + XCFramework (scripts/build-core.sh) and then the app; apple/ → the
# iOS app only. Then it reinstalls and relaunches Zoen in the simulator, so edits show up
# on screen within seconds.
#
#   start:  nohup scripts/watch.sh > build/watch.log 2>&1 &
#   stop:   scripts/watch.sh stop
#   status: tail -f build/watch.log
#
# Relaunch arguments live in build/watch.args, one per line (e.g. "-RodaOpen" / "paraty").
# Edit that file and touch any source file to relaunch with the new arguments.
# WATCH_DEVICE picks the simulator (default: the booted one). WATCH_MAC=1 also rebuilds
# and reopens the Mac app.
set -uo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
PIDF=build/watch.pid
mkdir -p build

if [[ "${1:-}" == stop ]]; then
  [[ -f $PIDF ]] && kill "$(cat $PIDF)" 2>/dev/null && echo "watch stopped" || echo "watch not running"
  pkill -f "fswatch.*$ROOT/apple" 2>/dev/null
  rm -f $PIDF
  exit 0
fi

if [[ -f $PIDF ]] && kill -0 "$(cat $PIDF)" 2>/dev/null; then echo "already running (pid $(cat $PIDF))"; exit 1; fi
echo $$ > $PIDF
trap 'pkill -f "fswatch.*$ROOT/apple" 2>/dev/null; rm -f $PIDF' EXIT

DEV=${WATCH_DEVICE:-booted}
BUNDLE=xyz.tironi.zoen
APP=build/DerivedData/Build/Products/Debug-iphonesimulator/Zoen.app
MACAPP=build/DerivedData/Build/Products/Debug/Zoen.app
FSWATCH=$(command -v fswatch || echo /opt/homebrew/bin/fswatch)
[[ -f build/watch.args ]] || printf '%s\n' -AppleLanguages '(en)' -AppleLocale en_US > build/watch.args

log() { echo "[$(date +%H:%M:%S)] $*"; }

relaunch() {
  local args=()
  while IFS= read -r l; do [[ -n "$l" ]] && args+=("$l"); done < build/watch.args
  xcrun simctl install "$DEV" "$APP" &&
    xcrun simctl launch --terminate-running-process "$DEV" $BUNDLE "${args[@]}" >/dev/null &&
    log "▶ relaunched on simulator (${args[*]})"
}

xbuild() { # scheme destination
  xcodebuild -project apple/Zoen.xcodeproj -scheme "$1" -configuration Debug -destination "$2" \
    -derivedDataPath build/DerivedData build > "build/watch-$1.log" 2>&1
  if grep -q "BUILD SUCCEEDED" "build/watch-$1.log"; then return 0; fi
  log "✗ $1 build failed:"; grep -E "^/.*error:" "build/watch-$1.log" | sort -u | head -15 | sed 's|'"$ROOT"'/||'
  return 1
}

cycle() { # $1 = 1 when crates/ changed
  local t0=$SECONDS
  if [[ "$1" == 1 ]]; then
    log "⚙ Rust changed → rebuilding core + XCFramework"
    scripts/build-core.sh > build/watch-core.log 2>&1 || { log "✗ Rust core failed (see build/watch-core.log)"; tail -5 build/watch-core.log; return; }
  fi
  log "⚙ building iOS app…"
  (cd apple && xcodegen generate --quiet) || { log "✗ xcodegen failed"; return; }
  xbuild ZoeniOS "platform=iOS Simulator,name=iPhone 17 Pro" || return
  relaunch
  log "✓ on screen in $((SECONDS - t0))s"
  if [[ "${WATCH_MAC:-0}" == 1 ]] && xbuild ZoenMac "platform=macOS,arch=arm64"; then
    pkill -x Zoen; sleep 1; open -n "$MACAPP" --args -ApplePersistenceIgnoreState YES && log "▶ Mac app reopened"
  fi
}

log "watching apple/ and crates/ (device: $DEV) — stop with scripts/watch.sh stop"
cycle 0

# fswatch batches events; we debounce further (1 s of quiet; bash 3.2 has integer read timeouts) so a whole sync lands as one build.
"$FSWATCH" -r -l 0.4 \
  -e '\.xcodeproj' -e 'xcuserdata' -e 'RodaFFI\.xcframework' -e 'RodaFFI\.generated\.swift' \
  -e '/\.[^/]*$' -e '\.swp$' -e '~$' \
  "$ROOT/apple" "$ROOT/crates" |
while IFS= read -r path; do
  rust=0; n=1
  [[ "$path" == "$ROOT/crates/"* ]] && rust=1
  while IFS= read -r -t 1 more; do
    n=$((n + 1)); [[ "$more" == "$ROOT/crates/"* ]] && rust=1
  done
  log "Δ $n change(s): ${path#$ROOT/}$([[ $n -gt 1 ]] && echo " +$((n - 1))")"
  cycle $rust
done
