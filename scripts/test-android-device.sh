#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../android"

collect_evidence() {
  local test_status=$?
  trap - EXIT
  set +e
  local evidence_output=app/build/outputs/device-evidence
  mkdir -p "$evidence_output"
  python3 - <<'PY' > "$evidence_output/collection.log" 2>&1
from pathlib import Path
import shutil

additional_output = Path("app/build/outputs/connected_android_test_additional_output")
evidence_output = Path("app/build/outputs/device-evidence")
png_signature = b"\x89PNG\r\n\x1a\n"
copied = 0
if not additional_output.is_dir():
    print("AGP did not produce additional instrumentation output.")
else:
    for source in sorted(additional_output.rglob("*")):
        if not source.is_file():
            continue
        relative = source.relative_to(additional_output)
        if source.suffix.lower() == ".png":
            with source.open("rb") as image:
                if image.read(len(png_signature)) != png_signature:
                    print(f"Rejected PNG without a valid signature: {relative}")
                    continue
        destination = evidence_output / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
        copied += 1
        print(f"Collected {relative}")
print(f"Collected {copied} instrumentation evidence files.")
PY
  local collection_status=$?
  cat "$evidence_output/collection.log"
  if (( collection_status != 0 )); then
    printf 'Evidence collection failed with status %s; preserving test status %s.\n' "$collection_status" "$test_status" >&2
  fi
  exit "$test_status"
}

# Do not upload captures left by a previous invocation if this run fails before testing.
rm -rf app/build/outputs/device-evidence app/build/outputs/connected_android_test_additional_output
trap collect_evidence EXIT
./gradlew :app:connectedDebugAndroidTest "$@"
