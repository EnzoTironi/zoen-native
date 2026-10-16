#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
output="$(mktemp -d "${TMPDIR:-/tmp}/zoen-message-mutations.XXXXXX")"
trap 'rm -rf "$output"' EXIT
xcrun swiftc -parse-as-library -swift-version 6 \
  "$root/apple/Shared/Features/MessageMutationDraft.swift" \
  "$root/apple/Tests/MessageMutationDraftJourney.swift" \
  -o "$output/message-mutations"
"$output/message-mutations"
