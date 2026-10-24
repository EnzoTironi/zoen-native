#!/usr/bin/env python3
"""Offline preparation of an exact owned PhotoCraft source; no build or activation."""
import argparse
import hashlib
import importlib.util
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


def browser_recipe():
    spec = importlib.util.spec_from_file_location("photo_browser_input", HERE / "browser_input.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def prepare(directory, browser=False, eframe_crate=None, egui_crate=None):
    if directory.is_symlink():
        raise ValueError("Refusing a symlinked source directory")
    directory = directory.resolve(strict=True)
    if git(directory, "rev-parse", "HEAD") != REVISION:
        raise ValueError("Refusing unpinned PhotoCraft source")
    if git(directory, "status", "--porcelain", "--untracked-files=all"):
        raise ValueError("Refusing dirty source; use a separate clean owned checkout")
    target = directory / "crates/zoen-host"
    receipt = directory / "zoen-photocraft-source.json"
    if target.exists() or receipt.exists() or (directory / "crates/zoen-web").exists() or (directory / "vendor").exists():
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
    if manifest.get("features") != {"browser": ["dep:photocraft-ui-egui", "dep:egui"]} or set(manifest["dependencies"]) != {
        "photocraft-doc", "photocraft-engine", "photocraft-format", "photocraft-io",
        "serde", "serde_json", "sha2", "photocraft-ui-egui", "egui",
    } or any(manifest["dependencies"][name].get("optional") is not True for name in ("photocraft-ui-egui", "egui")):
        raise ValueError("Review changed headless dependency roots before preparation")
    copies = [(p, target / p.relative_to(source)) for p in files]
    inputs = [Path(__file__).resolve(), HERE / "upstream.json", *files]
    foundation = None
    patches = []
    if browser:
        foundation = browser_recipe()
        inputs.extend(foundation.inputs())
        patches = [HERE / "patches/0001-managed-session.patch",
                   HERE / "patches/0002-managed-preview.patch"]
        for p in sorted((HERE / "browser").rglob("*")):
            if p.is_symlink():
                raise ValueError("Refusing symlinked browser input")
            if p.is_file():
                copies.append((p, directory / "crates/zoen-web" / p.relative_to(HERE / "browser")))
                inputs.append(p)
        layout = HERE.parent / "gridcraft-ui/browser/src/layout.rs"
        copies.append((layout, directory / "crates/zoen-web/src/layout.rs"))
        inputs.extend([layout, *patches])
    if any(p.is_symlink() or not p.is_file() for p in inputs):
        raise ValueError("Refusing non-regular recipe input")
    hashes = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs}
    upstream = {p: hashlib.sha256((directory / p).read_bytes()).hexdigest() for p in SOURCE_MANIFESTS}
    result = {"schema": 1, "upstream": REVISION, "upstreamTree": git(directory, "rev-parse", "HEAD^{tree}"),
              "profile": CONFIG["profile"], "inputs": hashes, "sourceManifests": upstream,
              "browserTarget": CONFIG["browserTarget"], "wasiQualified": False,
              "compiled": False, "runtimeExecuted": False, "appAvailable": False}
    # Verify shared archives before modifying the owned tree. No cache is patched.
    opened = foundation.archives(eframe_crate, egui_crate) if foundation else []
    try:
        for patch in patches:
            subprocess.run(["git", "apply", "--check", str(patch)], cwd=directory, check=True)
        for p, destination in copies:
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(p, destination)
        if foundation:
            foundation.apply(directory, opened)
            for patch in patches:
                subprocess.run(["git", "apply", str(patch)], cwd=directory, check=True)
            result["crateArchives"] = {name: hashlib.sha256(path.read_bytes()).hexdigest()
                for name, path in [("eframe", eframe_crate), ("egui", egui_crate)]}
        result["browserPrepared"] = browser
        receipt.write_text(json.dumps(result, indent=2) + "\n")
    finally:
        for archive in opened:
            archive.close()
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", type=Path, required=True)
    parser.add_argument("--browser", action="store_true")
    parser.add_argument("--eframe-crate", type=Path)
    parser.add_argument("--egui-crate", type=Path)
    args = parser.parse_args()
    try:
        result = prepare(args.upstream, args.browser, args.eframe_crate, args.egui_crate)
    except (ValueError, OSError, KeyError, subprocess.SubprocessError) as error:
        parser.exit(1, str(error) + "\n")
    print(json.dumps({"upstream": result["upstream"], "inputs": len(result["inputs"]),
                      "compiled": False, "runtimeExecuted": False, "appAvailable": False}))


if __name__ == "__main__":
    main()
