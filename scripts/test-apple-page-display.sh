#!/usr/bin/env bash
set -euo pipefail
task_root="$(cd "$(dirname "$0")/.." && pwd)"
task_output="$(mktemp -d "${TMPDIR:-/tmp}/zoen-page-display-test.XXXXXX")"
trap 'rm -rf "$task_output"' EXIT
xcrun swiftc -parse-as-library -swift-version 6 \
  "$task_root/apple/Shared/Features/Pages/PageDisplayText.swift" \
  "$task_root/apple/Tests/PageDisplayTextJourney.swift" \
  -framework Foundation -o "$task_output/page-display-journey"
"$task_output/page-display-journey"
