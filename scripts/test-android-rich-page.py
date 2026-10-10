#!/usr/bin/env python3
"""Repeat the unchanged rich-page journey, then collect separate phase evidence."""

import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import tarfile


root = Path(__file__).resolve().parent.parent
output = root / "android/app/build/outputs/device-evidence/rich-page-stress"
output.mkdir(parents=True, exist_ok=True)
devices = subprocess.check_output(["adb", "devices"], text=True)
serials = re.findall(r"^([^\s]+)\s+device$", devices, re.MULTILINE)
if len(serials) != 1:
    raise SystemExit(f"Expected exactly one ready Android device; found {serials}")
adb = ["adb", "-s", serials[0]]


def device(*args):
    return subprocess.check_output(adb + list(args), text=True, timeout=20).strip()


api = int(device("shell", "getprop", "ro.build.version.sdk"))
package = "xyz.tironi.zoen"
runner = package + ".test/androidx.test.runner.AndroidJUnitRunner"
journey = package + ".ParityJourneysTest#richFormattingDraftUndoSaveAndOldVersionPreviewUseTheNativeUi"
hashes = {}
apks = {
    package: root / "android/app/build/outputs/apk/debug/app-debug.apk",
    package + ".test": root / "android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk",
}
for app, apk in apks.items():
    if not apk.is_file():
        raise SystemExit(f"Build the debug and instrumentation APKs first: missing {apk}")
    subprocess.run(adb + ["install", "-r", "-t", str(apk)], check=True, timeout=60)
    installed = device("shell", "pm", "path", app).removeprefix("package:")
    if not installed.startswith("/data/app/") or "\n" in installed:
        raise SystemExit(f"Unexpected installed APK path for {app}")
    hashes[app] = device("shell", "sha256sum", installed).split()[0]
    if hashes[app] != hashlib.sha256(apk.read_bytes()).hexdigest():
        raise SystemExit(f"Installed APK differs from the built artifact: {app}")

record = {"api": api, "serial": serials[0], "installed_apk_sha256": hashes, "runs": []}
summary = output / "summary.json"


def run(number, evidence=False):
    log = output / f"run-{number}.log"
    if log.exists():
        raise SystemExit(f"Preserve existing rich-page stress evidence: {log}")
    args = ["shell", "am", "instrument", "-w", "-r", "-e", "class", journey]
    if evidence:
        args += ["-e", "zoenFormattingEvidence", "true", "-e", "additionalTestOutputDir", "/data/user/0/xyz.tironi.zoen/cache/rich-page-stress-evidence"]
    with log.open("w") as stream:
        result = subprocess.run(adb + args + [runner], stdout=stream, stderr=subprocess.STDOUT, timeout=120)
    text = log.read_text()
    passed = result.returncode == 0 and re.search(r"OK\s*\(1 test\)", text) is not None and "FAILURES!!!" not in text
    record["runs"].append({"number": number, "phase_captures": evidence, "passed": passed, "log_sha256": hashlib.sha256(log.read_bytes()).hexdigest()})
    summary.write_text(json.dumps(record, indent=2) + "\n")
    print(f"API {api}, rich-page repeat {number}: {'passed' if passed else 'FAILED'}", flush=True)
    if not passed:
        raise SystemExit(f"Rich-page repeat {number} failed; see {log}")


device("shell", "am", "force-stop", "com.android.cli.interact.instrumentation")
for number in range(1, 11):
    run(number)
device("shell", "run-as", package, "mkdir", "-p", "cache/rich-page-stress-evidence")
run("visual", evidence=True)
archive = subprocess.check_output(adb + ["exec-out", "run-as", package, "tar", "-cf", "-", "-C", "cache/rich-page-stress-evidence", "."], timeout=20)
with tarfile.open(fileobj=io.BytesIO(archive)) as bundle:
    for entry in bundle.getmembers():
        if not entry.isfile():
            continue
        relative = Path(entry.name)
        if relative.is_absolute() or ".." in relative.parts:
            raise SystemExit(f"Invalid evidence path: {entry.name}")
        data = bundle.extractfile(entry).read()
        if relative.suffix == ".png" and not data.startswith(b"\x89PNG\r\n\x1a\n"):
            raise SystemExit(f"Invalid evidence PNG: {entry.name}")
        destination = output / "visual" / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
