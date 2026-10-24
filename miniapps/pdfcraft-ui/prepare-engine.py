#!/usr/bin/env python3
"""Prepare an exact owned PdfCraft checkout without compiling or activating an App."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tomllib

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CONFIG = json.loads((HERE / "upstream.json").read_text())


def git(directory, *arguments):
    return subprocess.check_output(["git", "-C", str(directory), *arguments], text=True).strip()


def prepare(directory, managed=False):
    if directory.is_symlink():
        raise ValueError("Refusing a symlinked source directory")
    directory = directory.resolve(strict=True)
    if git(directory, "rev-parse", "HEAD") != CONFIG["revision"]:
        raise ValueError("Refusing an unpinned PdfCraft checkout")
    if git(directory, "status", "--porcelain", "--untracked-files=all"):
        raise ValueError("Refusing dirty source; use a clean owned checkout")
    for name, expected in CONFIG["sourceManifests"].items():
        path = directory / name
        if path.is_symlink() or not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError("Changed pinned source manifest: " + name)
    workspace = tomllib.loads((directory / "Cargo.toml").read_text())
    if "crates/*" not in workspace["workspace"]["members"]:
        raise ValueError("Upstream no longer discovers the managed crate")
    for name, revision in CONFIG["dependencyPins"].items():
        if workspace["workspace"]["dependencies"][name].get("rev") != revision:
            raise ValueError("Changed dependency revision: " + name)
    source = HERE / ("host-bridge" if managed else "inspection")
    files = sorted(source.rglob("*"))
    if any(path.is_symlink() for path in files):
        raise ValueError("Refusing symlinked inspection inputs")
    files = [path for path in files if path.is_file()]
    if not files or any(path.name == "Cargo.lock" or "target" in path.relative_to(source).parts for path in files):
        raise ValueError("Refusing generated or empty inspection inputs")
    target = directory / ("crates/zoen-host" if managed else "crates/zoen-inspection")
    receipt = directory / "zoen-pdfcraft-source.json"
    if target.exists() or receipt.exists():
        raise ValueError("Refusing existing prepared output")
    inputs = [Path(__file__).resolve(), HERE / "upstream.json", *files]
    structure = ROOT / "crates/miniapps/src/pdfcraft/structure.rs"
    if managed:
        if structure.is_symlink() or not structure.is_file():
            raise ValueError("Missing shared structural source")
        inputs.append(structure)
    result = {"schema": 1, "upstream": CONFIG["revision"],
              "upstreamTree": git(directory, "rev-parse", "HEAD^{tree}"),
              "mode": "managed-binary-native" if managed else "actual-engine-and-editor-inspection",
              "inputs": {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs},
              "sourceManifests": CONFIG["sourceManifests"],
              "profileChosen": False, "compiled": False, "runtimeExecuted": False, "appAvailable": False}
    # Original vendor patches, fonts, renderer and engine remain byte-for-byte upstream.
    shutil.copytree(source, target)
    if managed:
        shutil.copyfile(structure, target / "src/structure.rs")
    receipt.write_text(json.dumps(result, indent=2) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", type=Path, required=True)
    parser.add_argument("--managed", action="store_true")
    args = parser.parse_args()
    try:
        result = prepare(args.upstream, args.managed)
    except (ValueError, OSError, KeyError, subprocess.SubprocessError) as error:
        parser.exit(1, str(error) + "\n")
    print(json.dumps({"upstream": result["upstream"], "inputs": len(result["inputs"]),
                      "compiled": False, "runtimeExecuted": False, "appAvailable": False}))


if __name__ == "__main__":
    main()
