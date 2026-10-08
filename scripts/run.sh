#!/usr/bin/env bash
# Gera o projeto Xcode, compila e abre o Zoen no simulador do iPhone e no Mac.
#   scripts/run.sh            → iOS + Mac
#   scripts/run.sh ios        → só iOS
#   scripts/run.sh mac        → só Mac
#   SIM="iPhone 17 Pro" scripts/run.sh ios
set -euo pipefail
cd "$(dirname "$0")/.."
WHAT="${1:-all}"
SIM="${SIM:-iPhone 17 Pro}"
DD="$(pwd)/build/DerivedData"

[[ -d apple/Packages/RodaCore/RodaFFI.xcframework ]] || scripts/build-core.sh
(cd apple && xcodegen generate --quiet)

if [[ "$WHAT" == "all" || "$WHAT" == "ios" ]]; then
  echo "▸ xcodebuild ZoeniOS (simulador: $SIM)"
  xcodebuild -project apple/Zoen.xcodeproj -scheme ZoeniOS -configuration Debug \
    -destination "platform=iOS Simulator,name=$SIM" -derivedDataPath "$DD" \
    -quiet build
  UDID=$(xcrun simctl list devices available | grep -F "$SIM (" | head -1 | sed -E 's/.*\(([0-9A-F-]{36})\).*/\1/')
  xcrun simctl boot "$UDID" 2>/dev/null || true
  open -a Simulator
  xcrun simctl install "$UDID" "$DD/Build/Products/Debug-iphonesimulator/Zoen.app"
  xcrun simctl launch "$UDID" xyz.tironi.zoen "$@" >/dev/null
  echo "✓ Zoen rodando no $SIM"
fi

if [[ "$WHAT" == "all" || "$WHAT" == "mac" ]]; then
  echo "▸ xcodebuild ZoenMac"
  xcodebuild -project apple/Zoen.xcodeproj -scheme ZoenMac -configuration Debug \
    -destination "platform=macOS,arch=arm64" -derivedDataPath "$DD" -quiet build
  open "$DD/Build/Products/Debug/Zoen.app"
  echo "✓ Zoen rodando no Mac"
fi
