#!/usr/bin/env bash
# Gera o projeto Xcode, compila e abre o Zoen no simulador do iPhone e no Mac.
#   scripts/run.sh            → iOS + Mac
#   scripts/run.sh ios        → só iOS
#   scripts/run.sh mac        → só Mac
#   SIM="iPhone 17 Pro" scripts/run.sh ios
set -euo pipefail
cd "$(dirname "$0")/.."
WHAT="${1:-all}"
[[ $# -gt 0 ]] && shift
SIM="${SIM:-iPhone 17 Pro}"
DD="$(pwd)/build/DerivedData"

[[ -d apple/Packages/RodaCore/RodaFFI.xcframework ]] || scripts/build-core.sh
(cd apple && xcodegen generate --quiet)

if [[ "$WHAT" == "all" || "$WHAT" == "ios" ]]; then
  # O simulador pedido, ou o primeiro iPhone com iOS ≥ 26 (o mínimo do projeto) instalado.
  LINE=$(xcrun simctl list devices available | awk -v want="$SIM (" '
    /^-- iOS / { split($3, v, "."); ok = v[1] >= 26; next }
    /^-- /     { ok = 0; next }
    ok && /iPhone/ { if (index($0, want) && !hit) hit = $0; if (!any) any = $0 }
    END { print (hit ? hit : any) }')
  [[ -n "$LINE" ]] || { echo "✗ nenhum simulador de iPhone com iOS 26+ instalado" >&2; exit 1; }
  UDID=$(sed -E 's/.*\(([0-9A-F-]{36})\).*/\1/' <<<"$LINE")
  echo "▸ xcodebuild ZoeniOS (simulador: $(sed -E 's/^ *(.*) \([0-9A-F-]{36}\).*/\1/' <<<"$LINE"))"
  xcodebuild -project apple/Zoen.xcodeproj -scheme ZoeniOS -configuration Debug \
    -destination "platform=iOS Simulator,id=$UDID" -derivedDataPath "$DD" \
    -quiet build
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
