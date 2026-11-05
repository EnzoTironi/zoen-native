#!/bin/sh
# Managed local Rust builds: shared compiler artifacts and bounded cache storage.
# Keep Rust pins, Cargo arguments and explicitly selected native targets intact.
set -eu
zoen_cargo_home="${CARGO_HOME:-$HOME/.cargo}"
PATH="$zoen_cargo_home/bin:$HOME/.local/bin:$PATH"
export PATH
if ! command -v mbx >/dev/null 2>&1; then
  printf '%s\n' 'Install mr-boxington: https://mr-boxington.jdx.dev/installation' >&2
  exit 127
fi
exec mbx "$@"
