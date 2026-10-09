#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../android"

collect_evidence() {
  local test_status=$?
  trap - EXIT
  set +e
  local output=app/build/outputs/device-evidence
  mkdir -p "$output/media"
  adb pull /sdcard/Android/data/xyz.tironi.zoen/files/evidence "$output/mcp" > "$output/collection.log" 2>&1
  adb pull /sdcard/Android/data/xyz.tironi.zoen/files/globe-evidence "$output/globe" >> "$output/collection.log" 2>&1
  for name in voice-review-cut image-native-ink image-saved-version; do
    if adb exec-out run-as xyz.tironi.zoen cat "cache/media-evidence/$name.png" > "$output/media/$name.partial" 2>> "$output/collection.log"; then
      mv "$output/media/$name.partial" "$output/media/$name.png"
    else
      rm -f "$output/media/$name.partial"
    fi
  done
  exit "$test_status"
}

trap collect_evidence EXIT
./gradlew :app:connectedDebugAndroidTest "$@"
