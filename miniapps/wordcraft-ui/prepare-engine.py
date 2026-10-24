#!/usr/bin/env python3
"""Copy the managed headless adapter into a clean, exact pinned owned WordCraft tree.

Offline source preparation only. This neither compiles nor advertises an available App.
Cargo must generate its real lockfile; no handwritten dependency resolution is produced.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

REVISION = "d1b0bfc3f9dd83126b162a42b667facecb003b7f"
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def git(directory, *args):
    return subprocess.check_output(["git", "-C", str(directory), *args], text=True).strip()


def prepare(directory, wasi=False):
    directory = directory.resolve(strict=True)
    if git(directory, "rev-parse", "HEAD") != REVISION:
        raise ValueError("Refusing unpinned WordCraft source")
    if git(directory, "status", "--porcelain", "--untracked-files=all"):
        raise ValueError("Refusing dirty source: use a separate clean owned checkout")
    destination = directory / "crates/zoen-host"
    receipt = directory / "zoen-wordcraft-source.json"
    if destination.exists() or receipt.exists():
        raise ValueError("Refusing existing managed output")
    # Validate ALL source paths before making any destination. Symlinked authored inputs
    # must not escape the reviewed recipe, and caches/outputs are never copied into it.
    inputs = []
    for source in sorted((HERE / "host-bridge").rglob("*")):
        if source.is_symlink():
            raise ValueError("Refusing symlinked adapter input")
        if source.is_file():
            if source.name == "Cargo.lock" or "target" in source.parts:
                raise ValueError("Refusing generated adapter input")
            inputs.append(source)
    patches = [HERE / "patches/0001-wasi-system-clock.patch",
               HERE / "patches/0002-wasi-native-only-helpers.patch"] if wasi else []
    for source in patches:
        subprocess.run(["git", "-C", str(directory), "apply", "--check", str(source)], check=True)
    hashes = {str(source.relative_to(ROOT)): hashlib.sha256(source.read_bytes()).hexdigest()
              for source in [Path(__file__).resolve(), *patches, *inputs]}
    for source in patches:
        subprocess.run(["git", "-C", str(directory), "apply", str(source)], check=True)
    for source in inputs:
        target = destination / source.relative_to(HERE / "host-bridge")
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    result = {"schema": 1, "upstream": REVISION, "adapterAbi": "zoen-host-v1",
              "profile": "plain-text-v1", "inputs": hashes,
              "wasiClocks": wasi, "compiled": False, "runtimeExecuted": False, "appAvailable": False}
    receipt.write_text(json.dumps(result, indent=2) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", type=Path, required=True)
    parser.add_argument("--wasi", action="store_true", help="Use the real WASI system clocks; browser branches stay unchanged")
    args = parser.parse_args()
    try:
        result = prepare(args.upstream, wasi=args.wasi)
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, str(error) + "\n")
    print(json.dumps({"upstream": result["upstream"], "inputs": len(result["inputs"]),
                      "compiled": False, "runtimeExecuted": False}))


if __name__ == "__main__":
    main()
