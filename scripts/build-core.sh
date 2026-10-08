#!/usr/bin/env bash
# Compila o núcleo Rust para iPhone, simulador e Mac (arm64), gera os bindings
# Swift com UniFFI e empacota tudo como apple/Packages/RodaCore/RodaFFI.xcframework.
#   scripts/build-core.sh              always
#   scripts/build-core.sh --if-stale   only when a core source is newer than the package
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "${1:-}" == "--if-stale" ]]; then
  STAMP=apple/Packages/RodaCore/RodaFFI.xcframework/Info.plist
  if [[ -f "$STAMP" && -z "$(find crates/roda-* Cargo.lock \( -name '*.rs' -o -name Cargo.toml -o -name Cargo.lock \) -newer "$STAMP" -print -quit)" ]]; then
    exit 0
  fi
  echo "▸ rebuilding the core for the app"
fi

PROFILE="${PROFILE:-release}"
DIR="$PROFILE"; [[ "$PROFILE" == "dev" ]] && DIR="debug"
export IPHONEOS_DEPLOYMENT_TARGET=26.0
# Não exporte MACOSX_DEPLOYMENT_TARGET: no Xcode 27/rustc 1.95 isso gera dylibs de
# proc-macro do host inválidas ("mis-aligned LINKEDIT"). O mínimo padrão do macOS linka bem.
unset MACOSX_DEPLOYMENT_TARGET

TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin)
for t in "${TARGETS[@]}"; do
  echo "▸ cargo build roda-ffi ($PROFILE, $t)"
  cargo build -q -p roda-ffi --profile "$PROFILE" --target "$t"
done

OUT=build/bindings
rm -rf "$OUT" && mkdir -p "$OUT/headers"
echo "▸ uniffi-bindgen → Swift"
cargo run -q -p uniffi-bindgen -- generate \
  --library "target/aarch64-apple-darwin/$DIR/libroda_ffi.dylib" \
  --language swift --out-dir "$OUT" 2>/dev/null
cp "$OUT/roda_ffiFFI.h" "$OUT/headers/"
cp "$OUT/roda_ffiFFI.modulemap" "$OUT/headers/module.modulemap"

PKG=apple/Packages/RodaCore
rm -rf "$PKG/RodaFFI.xcframework"
echo "▸ xcodebuild -create-xcframework"
xcodebuild -create-xcframework \
  -library "target/aarch64-apple-ios/$DIR/libroda_ffi.a" -headers "$OUT/headers" \
  -library "target/aarch64-apple-ios-sim/$DIR/libroda_ffi.a" -headers "$OUT/headers" \
  -library "target/aarch64-apple-darwin/$DIR/libroda_ffi.a" -headers "$OUT/headers" \
  -output "$PKG/RodaFFI.xcframework" >/dev/null
cp "$OUT/roda_ffi.swift" "$PKG/Sources/RodaCore/RodaFFI.generated.swift"
echo "✓ RodaCore pronto ($(du -sh "$PKG/RodaFFI.xcframework" | cut -f1))"
