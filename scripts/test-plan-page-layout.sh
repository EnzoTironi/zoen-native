#!/usr/bin/env bash
# Run the presentation journey against a Mac build's actual generated Core module.
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "Usage: scripts/test-plan-page-layout.sh <Mac build products directory>" >&2
  exit 64
fi

zoen_root="$(cd "$(dirname "$0")/.." && pwd)"
zoen_products="$1"
zoen_archive="$zoen_root/apple/Packages/RodaCore/RodaFFI.xcframework/macos-arm64/libroda_ffi.a"

if [[ "$(uname -m)" != arm64 ]]; then
  echo "This journey requires an arm64 Mac build." >&2
  exit 1
fi
for zoen_file in "$zoen_products/RodaCore.o" "$zoen_products/include/roda_ffiFFI.h" "$zoen_archive"; do
  [[ -f "$zoen_file" ]] || { echo "Missing build product: $zoen_file" >&2; exit 1; }
done

zoen_temporary="$(mktemp -d "${TMPDIR:-/tmp}/zoen-plan-journey.XXXXXX")"
trap 'rm -f "$zoen_temporary/plan-page-journey"; rmdir "$zoen_temporary"' EXIT

xcrun swiftc -swift-version 6 -parse-as-library -j2 -target arm64-apple-macosx26.0 \
  -I "$zoen_products" -I "$zoen_products/include" \
  "$zoen_root/apple/Shared/Features/Pages/PlanPageLayout.swift" \
  "$zoen_root/apple/Tests/PlanPageLayoutJourney.swift" \
  "$zoen_products/RodaCore.o" "$zoen_archive" \
  -framework Foundation -framework Security -framework SystemConfiguration -lc++ \
  -o "$zoen_temporary/plan-page-journey"

"$zoen_temporary/plan-page-journey"
