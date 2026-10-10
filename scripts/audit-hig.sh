#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOL="$ROOT/tools/hig-audit/node_modules/hig-doctor/dist/index.js"

case "${1:-}" in
  apple) SOURCE="$ROOT/apple" ;;
  web) SOURCE="$ROOT/packages/zoen-ui/src" ;;
  *) echo "Usage: scripts/audit-hig.sh apple|web" >&2; exit 2 ;;
esac
if [[ $# -ne 1 ]]; then
  echo "Usage: scripts/audit-hig.sh apple|web" >&2
  exit 2
fi
if [[ ! -f "$TOOL" ]]; then
  echo "Install the locked auditor: npm ci --prefix tools/hig-audit --ignore-scripts --no-audit --no-fund" >&2
  exit 2
fi

# Advisory JSON only. Do not absorb findings into a baseline or change sources.
exec node "$TOOL" "$SOURCE" "$ROOT/tools/hig-audit/references" \
  --json --no-config --no-baseline \
  --exclude '**/RodaFFI.generated.swift' \
  --exclude 'Tests/**' --exclude 'MacUITests/**'
