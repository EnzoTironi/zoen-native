#!/usr/bin/env python3
"""Materialize reviewed adapter sources into a clean, pinned owned upstream tree.

Preparation only: no compiler, network, dependency installation or resource enablement.
The generated source receipt identifies every copied authored input. Compilation and
actual execution need independent receipts. Never point this at a shared checkout.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

REVISION = "b8771d51fb4da60bd48c13cf313d6d6014dc5e66"
ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent


def git(directory, *args):
    return subprocess.check_output(["git", "-C", str(directory), *args], text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", type=Path, required=True)
    parser.add_argument("--browser", action="store_true", help="Include the managed WebGL/WASM shell")
    args = parser.parse_args()
    directory = args.upstream.resolve(strict=True)
    if git(directory, "rev-parse", "HEAD") != REVISION:
        raise SystemExit("Refusing unpinned GridCraft source")
    if git(directory, "status", "--porcelain", "--untracked-files=all"):
        raise SystemExit("Refusing dirty source: prepare a separate clean owned checkout")
    destination = directory / "crates/zoen-host"
    if destination.exists():
        raise SystemExit("Refusing an existing adapter directory")
    patch = HERE / "patches/0001-managed-command-policy.patch"
    subprocess.run(["git", "-C", str(directory), "apply", "--check", str(patch)], check=True)
    subprocess.run(["git", "-C", str(directory), "apply", str(patch)], check=True)
    inputs = {}

    def copy(source, target):
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        inputs[str(source.relative_to(ROOT))] = hashlib.sha256(source.read_bytes()).hexdigest()

    for source in sorted((HERE / "host-bridge").rglob("*")):
        if source.is_file():
            copy(source, destination / source.relative_to(HERE / "host-bridge"))
    if args.browser:
        browser = directory / "crates/zoen-web"
        if browser.exists():
            raise SystemExit("Refusing an existing browser adapter directory")
        for source in sorted((HERE / "browser").rglob("*")):
            if source.is_file():
                copy(source, browser / source.relative_to(HERE / "browser"))
    profile = ROOT / "crates/miniapps/src/gridcraft.rs"
    copy(profile, destination / "src/profile.rs")
    for source in sorted(profile.with_suffix("").rglob("*.rs")):
        copy(source, destination / "src/profile" / source.relative_to(profile.with_suffix("")))
    # The source checker watches this prepared tree + these exact hashes. Cargo
    # must generate/update its lockfile genuinely, never using handwritten entries.
    inputs[str(patch.relative_to(ROOT))] = hashlib.sha256(patch.read_bytes()).hexdigest()
    (directory / "zoen-gridcraft-source.json").write_text(json.dumps({
        "schema": 1, "upstream": REVISION, "adapterAbi": "zoen-host-v1",
        "inputs": inputs, "compiled": False, "runtimeExecuted": False,
    }, indent=2) + "\n")
    print(json.dumps({"prepared": str(destination), "upstream": REVISION, "inputs": len(inputs)}))


if __name__ == "__main__":
    main()
