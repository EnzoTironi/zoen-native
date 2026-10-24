#!/usr/bin/env python3
"""Offline preparation of an exact owned PhotoCraft source; no build or activation."""
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
REVISION = CONFIG["revision"]
SOURCE_MANIFESTS = ("Cargo.toml", "crates/engine/Cargo.toml", "crates/format/Cargo.toml",
                    "crates/io/Cargo.toml", "apps/photocraft-web/Cargo.toml")


def git(directory, *args):
    return subprocess.check_output(["git", "-C", str(directory), *args], text=True).strip()


def prepare(directory):
    if directory.is_symlink():
        raise ValueError("Refusing a symlinked source directory")
    directory = directory.resolve(strict=True)
    if git(directory, "rev-parse", "HEAD") != REVISION:
        raise ValueError("Refusing unpinned PhotoCraft source")
    if git(directory, "status", "--porcelain", "--untracked-files=all"):
        raise ValueError("Refusing dirty source; use a separate clean owned checkout")
    target = directory / "crates/zoen-host"
    receipt = directory / "zoen-photocraft-source.json"
    if target.exists() or receipt.exists():
        raise ValueError("Refusing existing managed output")
    source = HERE / "host-bridge"
    files = sorted(source.rglob("*"))
    if any(p.is_symlink() for p in files):
        raise ValueError("Refusing symlinked adapter inputs")
    files = [p for p in files if p.is_file()]
    if not files or any(p.name == "Cargo.lock" or "target" in p.relative_to(source).parts for p in files):
        raise ValueError("Refusing generated/empty adapter inputs")
    for name in SOURCE_MANIFESTS:
        if (directory / name).is_symlink() or not (directory / name).is_file():
            raise ValueError("Missing or symlinked upstream manifest")
    workspace = tomllib.loads((directory / "Cargo.toml").read_text())
    if "crates/*" not in workspace["workspace"]["members"]:
        raise ValueError("Upstream workspace no longer includes the managed crate")
    for name, expected in CONFIG["dependencyPins"].items():
        if workspace["workspace"]["dependencies"][name].get("rev") != expected:
            raise ValueError("Changed upstream dependency revision: " + name)
    manifest = tomllib.loads((source / "Cargo.toml").read_text())
    if manifest.get("features") or set(manifest["dependencies"]) != {
        "photocraft-doc", "photocraft-engine", "photocraft-format", "photocraft-io",
        "serde", "serde_json", "sha2",
    }:
        raise ValueError("Review changed headless dependency roots before preparation")
    inputs = [Path(__file__).resolve(), HERE / "upstream.json", *files]
    if any(p.is_symlink() or not p.is_file() for p in inputs):
        raise ValueError("Refusing non-regular recipe input")
    hashes = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs}
    upstream = {p: hashlib.sha256((directory / p).read_bytes()).hexdigest() for p in SOURCE_MANIFESTS}
    result = {"schema": 1, "upstream": REVISION, "upstreamTree": git(directory, "rev-parse", "HEAD^{tree}"),
              "profile": CONFIG["profile"], "inputs": hashes, "sourceManifests": upstream,
              "browserTarget": CONFIG["browserTarget"], "wasiQualified": False,
              "compiled": False, "runtimeExecuted": False, "appAvailable": False}
    # Check every recipe input before creating any managed output. Existing
    # upstream engine/UI/format code and the original lockfile stay unchanged.
    for p in files:
        destination = target / p.relative_to(source)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(p, destination)
    receipt.write_text(json.dumps(result, indent=2) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", type=Path, required=True)
    args = parser.parse_args()
    try:
        result = prepare(args.upstream)
    except (ValueError, OSError, KeyError, subprocess.SubprocessError) as error:
        parser.exit(1, str(error) + "\n")
    print(json.dumps({"upstream": result["upstream"], "inputs": len(result["inputs"]),
                      "compiled": False, "runtimeExecuted": False, "appAvailable": False}))


if __name__ == "__main__":
    main()
