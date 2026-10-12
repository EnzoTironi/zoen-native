#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
test_dir="$(mktemp -d "${TMPDIR:-/tmp}/zoen-money-input.XXXXXX")"
trap 'rm -rf "$test_dir"' EXIT

xcrun swiftc -swift-version 6 -parse-as-library -warnings-as-errors \
  -module-cache-path "$test_dir/module-cache" \
  "$project_dir/apple/Shared/DesignSystem/MoneyInput.swift" \
  "$project_dir/apple/Tests/MoneyInputJourney.swift" \
  -o "$test_dir/MoneyInputJourney"
"$test_dir/MoneyInputJourney"
