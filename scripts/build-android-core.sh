#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

PROFILE=dev
ABIS=arm64-v8a,x86_64
for arg in "$@"; do
  case "$arg" in
    --release) PROFILE=release ;;
    --debug) PROFILE=dev ;;
    --abis=*) ABIS="${arg#--abis=}" ;;
    *) echo "Unknown option: $arg" >&2; exit 2 ;;
  esac
done

command -v cargo >/dev/null || { echo "Install stable Rust with rustup." >&2; exit 1; }
command -v cargo-ndk >/dev/null || { echo "Install cargo-ndk: cargo install cargo-ndk --locked" >&2; exit 1; }
SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [[ -z "$SDK" && "$(uname -s)" == Darwin ]]; then SDK="$HOME/Library/Android/sdk"; fi
[[ -d "$SDK/ndk" ]] || { echo "Set ANDROID_HOME to an SDK with NDK 27 or newer installed." >&2; exit 1; }
export ANDROID_HOME="$SDK"
OUT=android/app/build/generated/roda
mkdir -p "$OUT/kotlin" "$OUT/jniLibs"
find "$OUT/kotlin" -name '*.kt' -type f -delete

# Bindgen runs on the host. Kotlin and every ABI use metadata from the same build.
cargo build --locked -q -p roda-ffi -p uniffi-bindgen
case "$(uname -s)" in
  Darwin) HOST_LIB=target/debug/libroda_ffi.dylib ;;
  Linux) HOST_LIB=target/debug/libroda_ffi.so ;;
  *) echo "Build the Android core on macOS or Linux." >&2; exit 1 ;;
esac
target/debug/uniffi-bindgen generate --library "$HOST_LIB" --language kotlin \
  --config android/uniffi.toml --out-dir "$OUT/kotlin" --no-format

IFS=',' read -ra ABI_LIST <<< "$ABIS"
NDK_ARGS=()
TARGETS=()
for abi in "${ABI_LIST[@]}"; do
  case "$abi" in
    arm64-v8a) TARGETS+=(aarch64-linux-android) ;;
    x86_64) TARGETS+=(x86_64-linux-android) ;;
    *) echo "Unsupported ABI: $abi (use arm64-v8a or x86_64)" >&2; exit 2 ;;
  esac
  NDK_ARGS+=(-t "$abi")
done
rustup target add "${TARGETS[@]}"
# NDK 27 needs explicit 16 KB ELF alignment for Android 15+ devices.
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"
export CARGO_TARGET_X86_64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"
cargo ndk "${NDK_ARGS[@]}" --platform 28 -o "$OUT/jniLibs" \
  build --locked -q -p roda-ffi --profile "$PROFILE"
echo "Android core ready ($ABIS, $PROFILE)."
