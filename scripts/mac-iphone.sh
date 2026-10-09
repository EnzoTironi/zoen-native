#!/usr/bin/env bash
# O app de iPhone rodando direto no Mac com Apple Silicon, em janela no formato de iPhone,
# sem simulador ("My Mac (Designed for iPhone)"). Ver docs/dev/iphone-no-mac.md.
#   scripts/mac-iphone.sh                  compila (config Device, relay de staging) e abre
#   ZOEN_TEAM=ABCDE12345 scripts/mac-iphone.sh
#                                          outro time (padrão: o primeiro Personal Team do Xcode)
#   ZOEN_RELAY=http://127.0.0.1:8787 scripts/mac-iphone.sh
#                                          outro servidor no lugar do staging (relay.tryzoen.com)
set -euo pipefail
cd "$(dirname "$0")/.."
[[ "$(uname -m)" == "arm64" ]] || { echo "✗ precisa de um Mac com Apple Silicon"; exit 1; }
DD="$(pwd)/build/DerivedData-macphone"
OUT="$(pwd)/build/ZoenPhone.app"

if [[ -z "${ZOEN_TEAM:-}" ]]; then
  ZOEN_TEAM="$(defaults read com.apple.dt.Xcode IDEProvisioningTeamByIdentifier 2>/dev/null \
    | awk -F' = ' '/teamID/ {gsub(/[ ";]/,"",$2); print $2; exit}')"
fi
[[ -n "${ZOEN_TEAM:-}" ]] || { echo "✗ nenhum time no Xcode: Settings > Accounts > adicione seu Apple ID"; exit 1; }

scripts/build-core.sh --if-stale
(cd apple && xcodegen generate --quiet)
echo "▸ xcodebuild Zoen-Device para o Mac (time $ZOEN_TEAM)"
xcodebuild -project apple/Zoen.xcodeproj -scheme Zoen-Device -configuration Device \
  -destination 'platform=macOS,arch=arm64,variant=Designed for iPad' -derivedDataPath "$DD" \
  -allowProvisioningUpdates DEVELOPMENT_TEAM="$ZOEN_TEAM" \
  ${ZOEN_RELAY:+ZOEN_RELAY_URL="$ZOEN_RELAY"} -quiet build

# Um .app de iOS não abre direto no macOS ("incorrect executable format"): o sistema
# espera o mesmo invólucro que o Xcode e a App Store usam (Wrapper/ + WrappedBundle).
osascript -e 'tell application id "xyz.tironi.zoen.dev" to quit' >/dev/null 2>&1 || true
rm -rf "$OUT" && mkdir -p "$OUT/Wrapper"
cp -R "$DD/Build/Products/Device-iphoneos/Zoen.app" "$OUT/Wrapper/"
ln -s Wrapper/Zoen.app "$OUT/WrappedBundle"
open "$OUT"
echo "✓ aberto: $OUT"
