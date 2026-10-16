#!/usr/bin/env bash
# Type-check the native Liquid Glass components against both deployment targets.
# The full app also needs the Rust XCFramework; this checks the shared design unit.
set -euo pipefail
cd "$(dirname "$0")/.."
src=apple/Shared/DesignSystem
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cat > "$work/Tokens.swift" <<'SWIFT'
import SwiftUI
enum InkPalette { static let paper = Color.white }
SWIFT
xcrun -sdk iphonesimulator swiftc -typecheck -target arm64-apple-ios26.0-simulator \
  "$src/GlassButton.swift" "$src/GlassCard.swift" "$work/Tokens.swift"
echo "swift ok: iOS"
xcrun -sdk macosx swiftc -typecheck -target arm64-apple-macos26.0 \
  "$src/GlassButton.swift" "$src/GlassCard.swift" "$work/Tokens.swift"
echo "swift ok: macOS"
