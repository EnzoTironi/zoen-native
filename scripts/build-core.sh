#!/usr/bin/env bash
# Compila o núcleo Rust para iPhone, simulador e Mac (arm64), gera os bindings
# Swift com UniFFI e empacota tudo como apple/Packages/ZoenCore/ZoenFFI.xcframework.
#   scripts/build-core.sh              always
#   scripts/build-core.sh --if-stale   only when a core source is newer than the package
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "${1:-}" == "--if-stale" ]]; then
  STAMP=apple/Packages/ZoenCore/ZoenFFI.xcframework/Info.plist
  if [[ -f "$STAMP" && -z "$(find crates/zoen-* Cargo.lock \( -name '*.rs' -o -name Cargo.toml -o -name Cargo.lock \) -newer "$STAMP" -print -quit)" ]]; then
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
  echo "▸ cargo build zoen-ffi ($PROFILE, $t)"
  if [[ "$t" == "aarch64-apple-darwin" ]]; then
    zoen_mac_cflags="$(printenv CFLAGS_aarch64-apple-darwin || true)"
    zoen_mac_cxxflags="$(printenv CXXFLAGS_aarch64-apple-darwin || true)"
    zoen_mac_cflags="${zoen_mac_cflags:-${CFLAGS_aarch64_apple_darwin:-${CFLAGS:-}}} -mmacosx-version-min=26.0"
    zoen_mac_cxxflags="${zoen_mac_cxxflags:-${CXXFLAGS_aarch64_apple_darwin:-${CXXFLAGS:-}}} -mmacosx-version-min=26.0"
    env "CFLAGS_${t}=$zoen_mac_cflags" "CXXFLAGS_${t}=$zoen_mac_cxxflags" \
      cargo build -q -p zoen-ffi --profile "$PROFILE" --target "$t"
  else
    cargo build -q -p zoen-ffi --profile "$PROFILE" --target "$t"
  fi
done

OUT=build/bindings
rm -rf "$OUT" && mkdir -p "$OUT/headers"
echo "▸ uniffi-bindgen → Swift"
cargo run -q -p uniffi-bindgen -- generate \
  --library "target/aarch64-apple-darwin/$DIR/libzoen_ffi.dylib" \
  --language swift --out-dir "$OUT" 2>/dev/null
cp "$OUT/zoen_ffiFFI.h" "$OUT/headers/"
cp "$OUT/zoen_ffiFFI.modulemap" "$OUT/headers/module.modulemap"

PKG=apple/Packages/ZoenCore
rm -rf "$PKG/ZoenFFI.xcframework"
echo "▸ xcodebuild -create-xcframework"
xcodebuild -create-xcframework \
  -library "target/aarch64-apple-ios/$DIR/libzoen_ffi.a" -headers "$OUT/headers" \
  -library "target/aarch64-apple-ios-sim/$DIR/libzoen_ffi.a" -headers "$OUT/headers" \
  -library "target/aarch64-apple-darwin/$DIR/libzoen_ffi.a" -headers "$OUT/headers" \
  -output "$PKG/ZoenFFI.xcframework" >/dev/null
cp "$OUT/zoen_ffi.swift" "$PKG/Sources/ZoenCore/ZoenFFI.generated.swift"
echo "✓ ZoenCore pronto ($(du -sh "$PKG/ZoenFFI.xcframework" | cut -f1))"
