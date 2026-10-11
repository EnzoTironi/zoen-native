#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../android"
command -v adb >/dev/null || { echo "Install Android SDK platform-tools." >&2; exit 1; }
if [[ "$(uname -s)" == Darwin && -z "${JAVA_HOME:-}" ]]; then
  export JAVA_HOME="$(/usr/libexec/java_home -v 17)"
fi
./gradlew :app:assembleDebug "$@"
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n xyz.tironi.zoen/.MainActivity
