#!/usr/bin/env bash
# Compila o núcleo Rust para iPhone, simulador e Mac (arm64), gera os bindings
# Swift com UniFFI e empacota tudo como apple/Packages/ClientCore/ClientFFI.xcframework.
#   scripts/build-core.sh              always
#   scripts/build-core.sh --if-stale   only when a core source is newer than the package
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "${1:-}" == "--if-stale" ]]; then
  STAMP=apple/Packages/ClientCore/ClientFFI.xcframework/Info.plist
  BINDINGS=apple/Packages/ClientCore/Sources/ClientCore/ClientFFI.generated.swift
  if [[ -f "$STAMP" && -f "$BINDINGS" && -z "$(find crates tools/uniffi-bindgen Cargo.toml Cargo.lock scripts/build-core.sh \( -name '*.rs' -o -name '*.toml' -o -name '*.html' -o -name '*.json' -o -name Cargo.lock -o -name build-core.sh \) -newer "$STAMP" -print -quit)" ]]; then
    exit 0
  fi
  echo "▸ rebuilding the core for the app"
fi

PROFILE="${PROFILE:-release}"
DIR="$PROFILE"; [[ "$PROFILE" == "dev" ]] && DIR="debug"
export IPHONEOS_DEPLOYMENT_TARGET=26.0
# Não exporte MACOSX_DEPLOYMENT_TARGET: no Xcode 27/rustc 1.95 isso gera dylibs de
# proc-macro do host inválidas ("mis-aligned LINKEDIT"). Aplique o mínimo do app
# às dependências C/C++ apenas na compilação do target macOS abaixo.
unset MACOSX_DEPLOYMENT_TARGET

TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin)
for t in "${TARGETS[@]}"; do
  echo "▸ cargo build client-core ($PROFILE, $t)"
  if [[ "$t" == "aarch64-apple-darwin" ]]; then
    zoen_mac_cflags="$(printenv CFLAGS_aarch64-apple-darwin || true)"
    zoen_mac_cxxflags="$(printenv CXXFLAGS_aarch64-apple-darwin || true)"
    zoen_mac_cflags="${zoen_mac_cflags:-${CFLAGS_aarch64_apple_darwin:-${CFLAGS:-}}} -mmacosx-version-min=26.0"
    zoen_mac_cxxflags="${zoen_mac_cxxflags:-${CXXFLAGS_aarch64_apple_darwin:-${CXXFLAGS:-}}} -mmacosx-version-min=26.0"
    env "CFLAGS_${t}=$zoen_mac_cflags" "CXXFLAGS_${t}=$zoen_mac_cxxflags" \
      cargo build -q -p client-core --profile "$PROFILE" --target "$t"
  else
    cargo build -q -p client-core --profile "$PROFILE" --target "$t"
  fi
done

OUT=build/bindings
rm -rf "$OUT" && mkdir -p "$OUT/headers"
echo "▸ uniffi-bindgen → Swift"
cargo run -q -p uniffi-bindgen -- generate \
  --library "target/aarch64-apple-darwin/$DIR/libclient_core.dylib" \
  --language swift --out-dir "$OUT" 2>/dev/null
cp "$OUT/client_coreFFI.h" "$OUT/headers/"
cp "$OUT/client_coreFFI.modulemap" "$OUT/headers/module.modulemap"

PKG=apple/Packages/ClientCore
rm -rf "$PKG/ClientFFI.xcframework"
echo "▸ xcodebuild -create-xcframework"
xcodebuild -create-xcframework \
  -library "target/aarch64-apple-ios/$DIR/libclient_core.a" -headers "$OUT/headers" \
  -library "target/aarch64-apple-ios-sim/$DIR/libclient_core.a" -headers "$OUT/headers" \
  -library "target/aarch64-apple-darwin/$DIR/libclient_core.a" -headers "$OUT/headers" \
  -output "$PKG/ClientFFI.xcframework" >/dev/null
cp "$OUT/client_core.swift" "$PKG/Sources/ClientCore/ClientFFI.generated.swift"
echo "✓ ClientCore pronto ($(du -sh "$PKG/ClientFFI.xcframework" | cut -f1))"
