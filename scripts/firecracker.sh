#!/usr/bin/env bash
# Firecracker, its jailer and a guest kernel for the microVM tier (ADR 0028, P1).
# Linux with /dev/kvm only. Idempotent; pinned and checksummed; installs under .tools/firecracker.
#   scripts/firecracker.sh up        download firecracker, jailer and the guest kernel
#   scripts/firecracker.sh image     build zoen-guestd (static) and the guest root filesystem
#   scripts/firecracker.sh browser-image   (sudo) the browser microVM's root filesystem: the base
#                                    image plus Alpine's Chromium, fonts and NSS tools
#   scripts/firecracker.sh cgroup    (sudo) create the cgroup VMs run under, with a total cap
#   scripts/firecracker.sh sweep     kill every VM still in that cgroup
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
# A static curl in the base image: what most tools use for HTTPS (CONNECT through the proxy).
CURL_VERSION="8.22.0"
CURL_SHA256="dfb02460ba2abe513087538f12a3cf79b74b64a5ea3787ce8ac0cdb11251f884"
ALPINE_VERSION="3.24.2"
ALPINE_SHA256="c5ca053cfe1d85c5b96dff8b9bc57045f7f184a30ffb6b65776409ca90388677"
# apk, to install packages into the browser image; packages themselves are verified against
# Alpine's signing keys (shipped in the minirootfs).
APK_STATIC_VERSION="3.0.8-r0"
APK_STATIC_SHA256="c8e2c88c13ba12a12269b79a3543e1190ff8c0ab0beb32b58cadfd5881c619e3"
BROWSER_PACKAGES="chromium font-dejavu font-liberation nss-tools"

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
  if [[ ! -x "$TOOLS/curl" ]]; then
    curl -fsSL -o "$tmp/curl.tar.xz" \
      "https://github.com/stunnel/static-curl/releases/download/$CURL_VERSION/curl-linux-x86_64-musl-$CURL_VERSION.tar.xz"
    echo "$CURL_SHA256  $tmp/curl.tar.xz" | sha256sum -c - >/dev/null
    tar xJf "$tmp/curl.tar.xz" -C "$tmp" curl
    install -m 0755 "$tmp/curl" "$TOOLS/curl"
  fi
  rm -rf "$tmp"
  "$TOOLS/firecracker" --version | sed -n 1p
}

# Alpine's minirootfs plus zoen-guestd as init and a static curl. Read-only in the VM; /work, /tmp, /run and
# /root are tmpfs. Rebuilt whenever zoen-guestd changes.
image() {
  up >/dev/null
  rustup target add x86_64-unknown-linux-musl >/dev/null 2>&1 || true
  cargo build -q -p zoen-guestd --release --target x86_64-unknown-linux-musl
  local bin="${CARGO_TARGET_DIR:-$ROOT/target}/x86_64-unknown-linux-musl/release/zoen-guestd"
  local tmp; tmp="$(mktemp -d)"
  mkdir -p "$tmp/root"
  tar xzf "$TOOLS/alpine.tar.gz" -C "$tmp/root"
  install -m 0755 "$bin" "$tmp/root/sbin/zoen-guestd"
  install -m 0755 "$TOOLS/curl" "$tmp/root/usr/bin/curl"
  mkdir -p "$tmp/root/work" "$tmp/root/run"
  truncate -s 64M "$tmp/rootfs.ext4"
  "$(command -v mkfs.ext4 || echo /sbin/mkfs.ext4)" -q -F -d "$tmp/root" "$tmp/rootfs.ext4"
  mv "$tmp/rootfs.ext4" "$TOOLS/rootfs.ext4"
  rm -rf "$tmp"
  echo "guest image: $TOOLS/rootfs.ext4"
}

# The browser microVM's image: the base image's contents plus Chromium. Packages come from
# Alpine's repositories for the same release, verified with Alpine's keys; root is needed for
# the package scripts (fontconfig cache and the like) and file ownership.
browser_image() {
  image >/dev/null
  local repo="https://dl-cdn.alpinelinux.org/alpine/v${ALPINE_VERSION%.*}"
  if [[ ! -x "$TOOLS/apk.static" ]]; then
    local t; t="$(mktemp -d)"
    curl -fsSL -o "$t/apk.apk" "$repo/main/x86_64/apk-tools-static-$APK_STATIC_VERSION.apk"
    echo "$APK_STATIC_SHA256  $t/apk.apk" | sha256sum -c - >/dev/null
    tar xzf "$t/apk.apk" -C "$t" sbin/apk.static 2>/dev/null
    install -m 0755 "$t/sbin/apk.static" "$TOOLS/apk.static"
    rm -rf "$t"
  fi
  local bin="${CARGO_TARGET_DIR:-$ROOT/target}/x86_64-unknown-linux-musl/release/zoen-guestd"
  local tmp; tmp="$(mktemp -d "$TOOLS/browser-build.XXXXXX")"
  sudo mkdir -p "$tmp/root"
  sudo tar xzf "$TOOLS/alpine.tar.gz" -C "$tmp/root"
  # shellcheck disable=SC2086
  sudo timeout -s KILL 900 "$TOOLS/apk.static" --root "$tmp/root" --keys-dir "$tmp/root/etc/apk/keys" \
    -X "$repo/main" -X "$repo/community" --no-cache --quiet add $BROWSER_PACKAGES >/dev/null
  sudo install -m 0755 "$bin" "$tmp/root/sbin/zoen-guestd"
  sudo install -m 0755 "$TOOLS/curl" "$tmp/root/usr/bin/curl"
  sudo mkdir -p "$tmp/root/work" "$tmp/root/run"
  local chromium; chromium="$(sudo awk '/^P:chromium$/{f=1} f&&/^V:/{print $0; exit}' "$tmp/root/lib/apk/db/installed")"
  sudo rm -rf "$tmp/root/var/cache/apk"/* "$tmp/root/usr/share/doc" "$tmp/root/usr/share/man"
  local mib; mib=$(( $(sudo du -sm "$tmp/root" | cut -f1) * 5 / 4 + 64 ))
  truncate -s "${mib}M" "$tmp/browser.ext4"
  sudo "$(command -v mkfs.ext4 || echo /sbin/mkfs.ext4)" -q -F -L zoen-browser -d "$tmp/root" "$tmp/browser.ext4"
  sudo chown "$(id -u):$(id -g)" "$tmp/browser.ext4"
  mv "$tmp/browser.ext4" "$TOOLS/browser.ext4"
  # Never delete through a mount (a leftover /dev or /proc bind would take the host's with it).
  if findmnt -rno TARGET | grep -q "^$tmp"; then
    echo "refusing to remove $tmp: something is mounted under it"; exit 1
  fi
  sudo rm -rf --one-file-system "$tmp"
  echo "browser image: $TOOLS/browser.ext4 (${mib} MiB, chromium ${chromium#V:})"
}

# The parent cgroup every VMM goes under: cpu, memory and pids for its children, a cap on
# the total (so no number of VMs can take the host down), owned by the caller so it can
# remove VM cgroups without root.
CGROUP="${ZOEN_SANDBOX_CGROUP:-zoen-sandbox}"
CGROUP_MEMORY_MAX="${ZOEN_SANDBOX_MEMORY_MAX:-3G}"
cgroup() {
  local cg="/sys/fs/cgroup/$CGROUP"
  sudo mkdir -p "$cg"
  echo "+cpu +memory +pids" | sudo tee /sys/fs/cgroup/cgroup.subtree_control >/dev/null
  echo "+cpu +memory +pids" | sudo tee "$cg/cgroup.subtree_control" >/dev/null
  echo "$CGROUP_MEMORY_MAX" | sudo tee "$cg/memory.max" >/dev/null
  echo 512 | sudo tee "$cg/pids.max" >/dev/null
  sudo chown "$(id -u):$(id -g)" "$cg" "$cg/cgroup.procs" "$cg/cgroup.subtree_control" "$cg/cgroup.threads"
  echo "cgroup $cg: memory.max=$(cat "$cg/memory.max") pids.max=$(cat "$cg/pids.max")"
}

sweep() {
  local n=0
  for procs in /sys/fs/cgroup/"$CGROUP"/*/cgroup.procs; do
    [[ -f "$procs" ]] || continue
    while read -r pid; do kill -9 "$pid" 2>/dev/null && n=$((n + 1)); done < "$procs"
    rmdir "$(dirname "$procs")" 2>/dev/null || true
  done
  echo "killed $n VMM processes"
}

env_exports() {
  echo "export ZOEN_FIRECRACKER=\"$TOOLS/firecracker\""
  echo "export ZOEN_JAILER=\"$TOOLS/jailer\""
  echo "export ZOEN_VMLINUX=\"$TOOLS/vmlinux\""
  echo "export ZOEN_ALPINE_TGZ=\"$TOOLS/alpine.tar.gz\""
  echo "export ZOEN_GUEST_ROOTFS=\"$TOOLS/rootfs.ext4\""
  if [[ -f "$TOOLS/browser.ext4" ]]; then echo "export ZOEN_BROWSER_ROOTFS=\"$TOOLS/browser.ext4\""; fi
  echo "export ZOEN_SANDBOX_CGROUP=\"$CGROUP\""
}

measure() {
  up >/dev/null
  [[ -r /dev/kvm && -w /dev/kvm ]] || { echo "/dev/kvm is not usable by $(id -un)"; exit 1; }
  timeout -s KILL 180 python3 "$ROOT/scripts/fc-measure.py" "$TOOLS" "${1:-10}"
}

case "${1:-}" in
  up) up ;;
  image) image ;;
  browser-image) browser_image ;;
  cgroup) cgroup ;;
  sweep) sweep ;;
  env) env_exports ;;
  measure) shift; measure "$@" ;;
  *) echo "usage: $0 up|image|browser-image|cgroup|sweep|env|measure [runs]"; exit 2 ;;
esac
