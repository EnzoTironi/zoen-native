#!/usr/bin/env bash
# Zoen no seu iPhone com Apple ID grátis (Personal Team). Ver docs/dev/instalar-no-iphone.md.
#   scripts/device.sh                  gera o projeto, confere que compila para iPhone e abre o Xcode
#   ZOEN_TEAM=ABCDE12345 scripts/device.sh install
#                                      assina com seu time, instala e abre no iPhone plugado
#   ZOEN_RELAY=http://SEU-MAC.local:8787 ZOEN_TEAM=… scripts/device.sh install
#                                      outro servidor no lugar do staging (relay.tryzoen.com)
set -euo pipefail
cd "$(dirname "$0")/.."
DD="$(pwd)/build/DerivedData-device"
XCF=apple/Packages/RodaCore/RodaFFI.xcframework

scripts/build-core.sh --if-stale
[[ -d "$XCF" ]] || scripts/build-core.sh
[[ -d "$XCF/ios-arm64" ]] || { echo "✗ $XCF sem a fatia ios-arm64; rode scripts/build-core.sh"; exit 1; }
(cd apple && xcodegen generate --quiet)

if [[ "${1:-}" == "install" ]]; then
  : "${ZOEN_TEAM:?defina ZOEN_TEAM com o Team ID do seu Personal Team (Xcode > Settings > Accounts)}"
  echo "▸ xcodebuild Zoen-Device (assinado, time $ZOEN_TEAM)"
  xcodebuild -project apple/Zoen.xcodeproj -scheme Zoen-Device -configuration Device \
    -destination 'generic/platform=iOS' -derivedDataPath "$DD" \
    -allowProvisioningUpdates DEVELOPMENT_TEAM="$ZOEN_TEAM" \
    ${ZOEN_RELAY:+ZOEN_RELAY_URL="$ZOEN_RELAY"} -quiet build
  APP="$DD/Build/Products/Device-iphoneos/Zoen.app"
  if [[ -z "${ZOEN_DEVICE:-}" ]]; then
    J="$(mktemp)"; xcrun devicectl list devices --json-output "$J" >/dev/null 2>&1 || true
    ZOEN_DEVICE="$(python3 -c 'import json,sys
d=json.load(open(sys.argv[1])).get("result",{}).get("devices",[])
# Só aparelhos de verdade: o simulador aparece aqui como transportType "sameMachine".
ios=[x for x in d if x.get("hardwareProperties",{}).get("deviceType")=="iPhone" and x.get("connectionProperties",{}).get("transportType") in ("wired","localNetwork")]
ok=sorted(ios, key=lambda x: (x["connectionProperties"].get("transportType")!="wired", x["connectionProperties"].get("tunnelState")!="connected"))
ok=[x["identifier"] for x in ok]
print(ok[0] if ok else "")' "$J" 2>/dev/null || true)"; rm -f "$J"
  fi
  DEV="${ZOEN_DEVICE:-}"
  [[ -n "$DEV" ]] || { echo "✗ nenhum iPhone conectado (plugue o cabo e desbloqueie)"; exit 1; }
  xcrun devicectl device install app --device "$DEV" "$APP"
  xcrun devicectl device process launch --device "$DEV" xyz.tironi.zoen.dev || \
    echo "! instalado; se não abriu, confie no desenvolvedor em Ajustes > Geral > VPN e Gerenciamento de Dispositivos"
  echo "✓ Zoen no iPhone"
else
  echo "▸ xcodebuild Zoen-Device (só confere que compila para iPhone, sem assinar)"
  xcodebuild -project apple/Zoen.xcodeproj -scheme Zoen-Device -configuration Device \
    -sdk iphoneos -destination 'generic/platform=iOS' -derivedDataPath "$DD" \
    CODE_SIGNING_ALLOWED=NO -quiet build
  echo "✓ compila para iPhone; abrindo o Xcode (escolha o seu Personal Team e o seu iPhone, depois ▶)"
  open apple/Zoen.xcodeproj
fi
