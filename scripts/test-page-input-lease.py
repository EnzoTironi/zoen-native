#!/usr/bin/env python3
"""Run the real Page lease against supplied, normally generated macOS Core FFI.

See scripts/tests/page-input-lease.md for inputs and the deliberately narrow scope.
This runner neither builds Rust nor implements the ENRM producer/registry.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import subprocess
import sys
import time


LEASE = "apps/zoen/modules/zoen-core/ios/ZoenPageInputLease.swift"
TEST = "apps/zoen/modules/zoen-core/tests/PageInputLeaseTests.swift"
PASS = "PASS Page input native lease: exact retention, private Apply/replay, invalidate, malformed IME, Discard, whole document"


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def command(args, output, name, timeout, env, cwd):
    log = output / (name + ".log")
    record = {"command": [str(arg) for arg in args], "timeoutSeconds": timeout}
    start = time.monotonic()
    with log.open("wb") as stream:
        proc = subprocess.Popen(
            args, stdout=stream, stderr=subprocess.STDOUT, env=env,
            cwd=cwd, start_new_session=True,
        )
        try:
            record["exitCode"] = proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGTERM)
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
                proc.wait()
            record.update(exitCode=proc.returncode, timedOut=True)
    record.update(seconds=round(time.monotonic() - start, 3), log=str(log), logSha256=digest(log))
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--client-swift", type=Path, required=True)
    parser.add_argument("--ffi-header", type=Path, required=True)
    parser.add_argument("--ffi-modulemap", type=Path, required=True)
    parser.add_argument("--host-library", type=Path, required=True)
    parser.add_argument("--checkpoint-header", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--compile-timeout", type=int, default=180)
    parser.add_argument("--test-timeout", type=int, default=60)
    args = parser.parse_args()
    if platform.system() != "Darwin":
        parser.error("Requires macOS Foundation/Objective-C interoperability and a macOS Core dylib")
    if not 1 <= args.compile_timeout <= 600 or not 1 <= args.test_timeout <= 300:
        parser.error("Compile timeout must be 1..600 seconds; test timeout must be 1..300 seconds")
    root = args.source_root.resolve(strict=True)
    source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    if source != args.source_sha:
        parser.error("Source checkout HEAD does not match the explicit source SHA")
    files = {
        "clientSwift": args.client_swift,
        "ffiHeader": args.ffi_header,
        "ffiModulemap": args.ffi_modulemap,
        "hostLibrary": args.host_library,
        "checkpointHeader": args.checkpoint_header,
        "lease": root / LEASE,
        "test": root / TEST,
    }
    files = {name: path.resolve(strict=True) for name, path in files.items()}
    for name, path in files.items():
        if not path.is_file() or path.stat().st_size == 0:
            parser.error(f"Missing or empty input: {name}")
    if files["ffiHeader"].name != "client_coreFFI.h":
        parser.error("Use the actual generated client_coreFFI.h")
    if files["checkpointHeader"].name != "ENRMDraftSnapshot.h":
        parser.error("Use the installed real ENRMDraftSnapshot.h")
    if files["hostLibrary"].suffix != ".dylib":
        parser.error("Use a normally compiled macOS dylib, not an iOS archive")
    # Bind the production lease and test bodies to the named commit, not local edits.
    for name, relative in (("lease", LEASE), ("test", TEST)):
        committed = subprocess.check_output(["git", "show", f"{source}:{relative}"], cwd=root)
        if files[name].read_bytes() != committed:
            parser.error(f"{name} differs from the named source commit")
    output = args.output.resolve()
    if output.is_relative_to(root) or any(path.is_relative_to(output) for path in files.values()):
        parser.error("Use a new external output directory, separate from all supplied inputs")
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    result = {
        "source": source, "status": "FAIL", "scope": "macOS Foundation + genuine Core FFI",
        "notCovered": ["UIKit", "ENRM producer/registry implementation", "keyboard/focus", "iOS/device"],
        "cargoStarted": False, "servicesCreated": False, "stages": [],
        "inputs": {name: {"path": str(path), "sha256": digest(path)} for name, path in files.items()},
    }
    try:
        copied = output / "inputs"
        copied.mkdir()
        ffi = copied / "ffi"
        ffi.mkdir()
        # Both C FFI files are copied verbatim; only the discovery filename changes.
        shutil.copyfile(files["ffiHeader"], ffi / "client_coreFFI.h")
        shutil.copyfile(files["ffiModulemap"], ffi / "module.modulemap")
        enrm = copied / "enrm"
        enrm.mkdir()
        shutil.copyfile(files["checkpointHeader"], enrm / "ENRMDraftSnapshot.h")
        (enrm / "module.modulemap").write_text(
            'module ReactNativeEnrichedMarkdown {\n  header "ENRMDraftSnapshot.h"\n  export *\n}\n'
        )
        for name, filename in (("clientSwift", "ClientFFI.generated.swift"),
                               ("lease", "ZoenPageInputLease.swift"),
                               ("test", "PageInputLeaseTests.swift")):
            shutil.copyfile(files[name], copied / filename)
        # A dylib's install name may be absolute: use the sealed original in place.
        # Never rewrite install names or re-sign supplied artifacts in this runner.
        binary = output / "PageInputLeaseTests"
        swiftc = subprocess.check_output(["xcrun", "--find", "swiftc"], text=True).strip()
        env = os.environ.copy()
        for name in list(env):
            if name.startswith("DYLD_") or name.startswith("ZOEN_"):
                env.pop(name)
        for stage, argv in (
            ("swift-version", [swiftc, "--version"]),
            ("library-dependencies", ["/usr/bin/otool", "-L", str(files["hostLibrary"])]),
        ):
            evidence = command(argv, output, stage, 15, env, output)
            result["stages"].append(evidence)
            if evidence["exitCode"] != 0 or evidence.get("timedOut"):
                raise RuntimeError(f"{stage} failed; inspect its preserved log")
        argv = [swiftc, "-parse-as-library", "-module-name", "PageInputLeaseQualification",
                "-module-cache-path", str(output / "module-cache"),
                "-I", str(ffi), "-I", str(enrm),
                str(copied / "ClientFFI.generated.swift"), str(copied / "ZoenPageInputLease.swift"),
                str(copied / "PageInputLeaseTests.swift"), str(files["hostLibrary"]),
                "-Xlinker", "-rpath", "-Xlinker", str(files["hostLibrary"].parent), "-o", str(binary)]
        evidence = command(argv, output, "compile", args.compile_timeout, env, output)
        result["stages"].append(evidence)
        if evidence["exitCode"] != 0 or evidence.get("timedOut"):
            raise RuntimeError("Real Swift compile/link failed; inspect compile.log")
        result["binarySha256"] = digest(binary)
        evidence = command([str(binary)], output, "test", args.test_timeout, env, output)
        result["stages"].append(evidence)
        lines = (output / "test.log").read_text(errors="replace").splitlines()
        if evidence["exitCode"] != 0 or evidence.get("timedOut") or lines.count(PASS) != 1:
            raise RuntimeError("Real PageInputLeaseTests failed or its completion marker is absent")
        result["status"] = "PASS"
    except (OSError, subprocess.SubprocessError, RuntimeError) as error:
        result["error"] = str(error)
    finally:
        result["inputsStable"] = all(
            path.is_file() and digest(path) == result["inputs"][name]["sha256"]
            for name, path in files.items()
        )
        if not result["inputsStable"]:
            result.update(status="FAIL", error="An explicit input changed during qualification")
        (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps({"status": result["status"], "result": str(output / "result.json")}))
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
