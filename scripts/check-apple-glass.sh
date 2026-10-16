#!/usr/bin/env bash
# Compiles the Wabi glass shader and type-checks its SwiftUI style on the iOS and macOS
# SDKs. The full app needs the Rust XCFramework; this checks the design-system unit alone,
# with the few colour tokens it reads stubbed.
set -euo pipefail
cd "$(dirname "$0")/.."
src=apple/Shared/DesignSystem
work=$(mktemp -d)
cat > "$work/Tokens.swift" <<'SWIFT'
import SwiftUI
extension Color { init(hex: String) { self = .green } }
enum Mascot { static let body = Color.green }
enum InkPalette { static let ink = Color.black; static let paper = Color.white }
enum Palette { static let surface = Color.white; static let textPrimary = Color.black; static let action = Color.green; static let onAction = Color.white }
SWIFT
for sdk in iphonesimulator macosx; do
  xcrun -sdk "$sdk" metal -c "$src/ZoenGlass.metal" -o "$work/glass-$sdk.air"
  echo "metal ok: $sdk"
done
xcrun -sdk iphonesimulator swiftc -typecheck -target arm64-apple-ios26.0-simulator \
  "$src/GlassButton.swift" "$src/ZoenGlass.swift" "$work/Tokens.swift"
echo "swift ok: iOS"
xcrun -sdk macosx swiftc -typecheck -target arm64-apple-macos26.0 \
  "$src/GlassButton.swift" "$src/ZoenGlass.swift" "$work/Tokens.swift"
echo "swift ok: macOS"
