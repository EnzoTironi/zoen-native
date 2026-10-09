#!/usr/bin/env bash
# gVisor (runsc) and a base root filesystem for the sandbox stand-in (ADR 0028, P0).
# Linux only, no root needed (runsc runs rootless). Idempotent; pinned and checksummed.
#   scripts/gvisor.sh up    download runsc and the alpine minirootfs under .tools/gvisor
#   scripts/gvisor.sh env   print the exports the journeys need (eval "$(scripts/gvisor.sh env)")
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"
TOOLS="$ROOT/.tools/gvisor"
GVISOR_RELEASE="release-20261005.0"
GVISOR_SHA256="bd4eb2ca0ee74f0fbbb5f5273ba011e79c56a29fbb047393b88b981cd3230ede"
ALPINE_VERSION="3.24.2"
ALPINE_SHA256="c5ca053cfe1d85c5b96dff8b9bc57045f7f184a30ffb6b65776409ca90388677"

up() {
  [[ "$(uname -s)" == Linux ]] || { echo "gVisor runs on Linux only"; exit 1; }
  mkdir -p "$TOOLS"
  local tmp; tmp="$(mktemp -d)"
  if [[ ! -x "$TOOLS/runsc" ]]; then
    curl -fsSL -o "$tmp/gvisor.tar.bz2" \
      "https://github.com/google/gvisor/releases/download/$GVISOR_RELEASE/gvisor-x86_64.tar.bz2"
    echo "$GVISOR_SHA256  $tmp/gvisor.tar.bz2" | sha256sum -c - >/dev/null
    # runsc needs its sidecars (gvisor-bin/) next to it.
    python3 -c "import tarfile,sys; tarfile.open(sys.argv[1]).extractall(sys.argv[2], filter='tar')" \
      "$tmp/gvisor.tar.bz2" "$TOOLS"
    chmod +x "$TOOLS/runsc"
  fi
  if [[ ! -f "$TOOLS/rootfs-base/etc/alpine-release" ]]; then
    curl -fsSL -o "$tmp/alpine.tar.gz" \
      "https://dl-cdn.alpinelinux.org/alpine/v${ALPINE_VERSION%.*}/releases/x86_64/alpine-minirootfs-$ALPINE_VERSION-x86_64.tar.gz"
    echo "$ALPINE_SHA256  $tmp/alpine.tar.gz" | sha256sum -c - >/dev/null
    rm -rf "$TOOLS/rootfs-base" && mkdir -p "$TOOLS/rootfs-base"
    tar xzf "$tmp/alpine.tar.gz" -C "$TOOLS/rootfs-base"
  fi
  rm -rf "$tmp"
  "$TOOLS/runsc" --version | sed -n 1p
}

env_exports() {
  echo "export ZOEN_RUNSC=\"$TOOLS/runsc\""
  echo "export ZOEN_SANDBOX_ROOTFS=\"$TOOLS/rootfs-base\""
}

case "${1:-}" in
  up) up ;;
  env) env_exports ;;
  *) echo "usage: $0 up|env"; exit 2 ;;
esac
