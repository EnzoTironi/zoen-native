#!/usr/bin/env bash
# Firecracker, its jailer and a guest kernel for the microVM tier (ADR 0028, P1).
# Linux with /dev/kvm only. Idempotent; pinned and checksummed; installs under .tools/firecracker.
#   scripts/firecracker.sh up        download firecracker, jailer and the guest kernel
#   scripts/firecracker.sh env       print the exports the journeys need
#   scripts/firecracker.sh measure   cold boot and snapshot-restore timings (scripts/fc-measure.py)
# Every VM this script starts runs under `timeout -s KILL`, so nothing can hang the host.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"
TOOLS="$ROOT/.tools/firecracker"
FC_VERSION="v1.17.0"
FC_SHA256="06094a1108ae9e82aa4c23a775aa92758f53f1175d422270d9d6162cb9ade558"
# Firecracker's CI guest kernel (6.1, built with their microVM config).
KERNEL_URL="https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.13/x86_64/vmlinux-6.1.141"
KERNEL_SHA256="b36a4a1b10f33b9cfdcde3d1a787d9c090556a3edb211cd06d1f3f9a6c7e8724"
ALPINE_VERSION="3.24.2"
ALPINE_SHA256="c5ca053cfe1d85c5b96dff8b9bc57045f7f184a30ffb6b65776409ca90388677"

up() {
  [[ "$(uname -s)" == Linux ]] || { echo "Firecracker runs on Linux only"; exit 1; }
  mkdir -p "$TOOLS"
  local tmp; tmp="$(mktemp -d)"
  if [[ ! -x "$TOOLS/firecracker" ]]; then
    curl -fsSL -o "$tmp/fc.tgz" \
      "https://github.com/firecracker-microvm/firecracker/releases/download/$FC_VERSION/firecracker-$FC_VERSION-x86_64.tgz"
    echo "$FC_SHA256  $tmp/fc.tgz" | sha256sum -c - >/dev/null
    tar xzf "$tmp/fc.tgz" -C "$tmp" --wildcards '*/firecracker-*-x86_64' '*/jailer-*-x86_64'
    install -m 0755 "$tmp/release-$FC_VERSION-x86_64/firecracker-$FC_VERSION-x86_64" "$TOOLS/firecracker"
    install -m 0755 "$tmp/release-$FC_VERSION-x86_64/jailer-$FC_VERSION-x86_64" "$TOOLS/jailer"
  fi
  if [[ ! -f "$TOOLS/vmlinux" ]]; then
    curl -fsSL -o "$tmp/vmlinux" "$KERNEL_URL"
    echo "$KERNEL_SHA256  $tmp/vmlinux" | sha256sum -c - >/dev/null
    mv "$tmp/vmlinux" "$TOOLS/vmlinux"
  fi
  if [[ ! -f "$TOOLS/alpine.tar.gz" ]]; then
    curl -fsSL -o "$tmp/alpine.tar.gz" \
      "https://dl-cdn.alpinelinux.org/alpine/v${ALPINE_VERSION%.*}/releases/x86_64/alpine-minirootfs-$ALPINE_VERSION-x86_64.tar.gz"
    echo "$ALPINE_SHA256  $tmp/alpine.tar.gz" | sha256sum -c - >/dev/null
    mv "$tmp/alpine.tar.gz" "$TOOLS/alpine.tar.gz"
  fi
  rm -rf "$tmp"
  "$TOOLS/firecracker" --version | sed -n 1p
}

env_exports() {
  echo "export ZOEN_FIRECRACKER=\"$TOOLS/firecracker\""
  echo "export ZOEN_JAILER=\"$TOOLS/jailer\""
  echo "export ZOEN_VMLINUX=\"$TOOLS/vmlinux\""
  echo "export ZOEN_ALPINE_TGZ=\"$TOOLS/alpine.tar.gz\""
}

measure() {
  up >/dev/null
  [[ -r /dev/kvm && -w /dev/kvm ]] || { echo "/dev/kvm is not usable by $(id -un)"; exit 1; }
  timeout -s KILL 180 python3 "$ROOT/scripts/fc-measure.py" "$TOOLS" "${1:-10}"
}

case "${1:-}" in
  up) up ;;
  env) env_exports ;;
  measure) shift; measure "$@" ;;
  *) echo "usage: $0 up|env|measure [runs]"; exit 2 ;;
esac
