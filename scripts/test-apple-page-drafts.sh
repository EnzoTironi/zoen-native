#!/usr/bin/env bash
set -euo pipefail
if [[ $# -ne 1 ]]; then
  echo 'Usage: scripts/test-apple-page-drafts.sh <current Mac DerivedData/Build/Products/Debug>' >&2
  exit 2
fi
task_root="$(cd "$(dirname "$0")/.." && pwd)"
products="$1"
for file in ZoenCore.o libzoen_ffi.a include/module.modulemap; do
  [[ -f "$products/$file" ]] || { echo "Build the current Mac app first: missing $products/$file" >&2; exit 2; }
done
[[ -d "$products/ZoenCore.swiftmodule" ]] || { echo 'Missing current ZoenCore.swiftmodule' >&2; exit 2; }
task_output="$(mktemp -d "${TMPDIR:-/tmp}/zoen-page-draft-test.XXXXXX")"
trap 'rm -rf "$task_output"' EXIT
xcrun swiftc -parse-as-library -swift-version 6 -D DEBUG \
  -I "$products" -I "$products/include" \
  "$task_root/apple/Shared/Features/Pages/PageDraftStore.swift" \
  "$task_root/apple/Shared/Sync/KeychainVault.swift" \
  "$task_root/apple/Tests/PageDraftStoreJourney.swift" \
  "$products/ZoenCore.o" "$products/libzoen_ffi.a" \
  -framework Foundation -framework CryptoKit -framework Security \
  -lc++ -lz -liconv -Xlinker -dead_strip -o "$task_output/page-draft-journey"
"$task_output/page-draft-journey"
