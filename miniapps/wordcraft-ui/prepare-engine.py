#!/usr/bin/env python3
"""Prepare an exact pinned owned WordCraft tree with reproducible managed inputs.

Offline preparation only. Cargo generates its genuine lockfile; this neither
compiles nor advertises an available App. Browser and WASI use separate trees.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess

REVISION = "d1b0bfc3f9dd83126b162a42b667facecb003b7f"
CODEC_REVISION = REVISION
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def git(directory, *args):
    return subprocess.check_output(["git", "-C", str(directory), *args], text=True).strip()


def browser_recipe():
    spec = importlib.util.spec_from_file_location("wordcraft_browser_input", HERE / "browser_input.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def prepare(directory, wasi=False, browser=False, eframe_crate=None, egui_crate=None):
    directory = directory.resolve(strict=True)
    if wasi and browser:
        raise ValueError("Use separate browser and WASI source trees")
    if git(directory, "rev-parse", "HEAD") != REVISION:
        raise ValueError("Refusing unpinned WordCraft source")
    if git(directory, "status", "--porcelain", "--untracked-files=all"):
        raise ValueError("Refusing dirty source: use a separate clean owned checkout")
    receipt = directory / "zoen-wordcraft-source.json"
    if any((directory / name).exists() for name in ("crates/zoen-host", "crates/zoen-web", "vendor")) or receipt.exists():
        raise ValueError("Refusing existing managed output")
    copies = [(source, Path("crates/zoen-host") / source.relative_to(HERE / "host-bridge"))
              for source in sorted((HERE / "host-bridge").rglob("*")) if source.is_file()]
    patches = [HERE / "patches/0001-wasi-system-clock.patch",
               HERE / "patches/0002-wasi-native-only-helpers.patch"] if wasi else []
    profile = ROOT / "crates/wordcraft-profile"
    copies.extend((source, Path("crates/zoen-word-profile") / source.relative_to(profile))
                  for source in sorted(profile.rglob("*")) if source.is_file())
    inputs = [Path(__file__).resolve()]
    foundation = None
    if browser:
        patches = [HERE / "patches/0003-managed-session.patch",
                   HERE / "patches/0005-browser-native-only-helpers.patch"]
        copies.extend((source, Path("crates/zoen-web") / source.relative_to(HERE / "browser"))
                      for source in sorted((HERE / "browser").rglob("*")) if source.is_file())
        layout = HERE.parent / "gridcraft-ui/browser/src/layout.rs"
        copies.append((layout, Path("crates/zoen-web/src/layout.rs")))
        foundation = browser_recipe()
        inputs.extend(foundation.inputs())
    inputs.extend(patches)
    inputs.extend(source for source, _ in copies)
    for source in inputs:
        if source.is_symlink() or not source.is_file():
            raise ValueError("Refusing non-regular or symlinked recipe input")
        if source.name == "Cargo.lock" or "target" in source.parts:
            raise ValueError("Refusing generated adapter input")
    for folder in [HERE / "host-bridge", *([HERE / "browser"] if browser else [])]:
        if any(p.is_symlink() for p in folder.rglob("*")):
            raise ValueError("Refusing symlinked adapter directory")
    profile_manifest = (profile / "Cargo.toml").read_text()
    for name, relative in [("wordcraft-doc", "../doc"), ("wordcraft-docx", "../docx")]:
        fixed = f'{name} = {{ git = "https://github.com/storytold/wordcraft", rev = "{CODEC_REVISION}" }}'
        if fixed not in profile_manifest:
            raise ValueError("Shared profile codec pin changed")
        profile_manifest = profile_manifest.replace(fixed, f'{name} = {{ path = "{relative}" }}')
    opened = []
    try:
        if foundation:
            opened = foundation.archives(eframe_crate, egui_crate)
        for patch in patches:
            subprocess.run(["git", "-C", str(directory), "apply", "--check", str(patch)], check=True)
        hashes = {str(source.relative_to(ROOT)): hashlib.sha256(source.read_bytes()).hexdigest()
                  for source in inputs}
        # All closed inputs were checked before making the first managed output.
        for patch in patches:
            subprocess.run(["git", "-C", str(directory), "apply", str(patch)], check=True)
        for source, relative in copies:
            target = directory / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
        (directory / "crates/zoen-word-profile/Cargo.toml").write_text(profile_manifest)
        if foundation:
            foundation.apply(directory, opened)
        result = {"schema": 1, "upstream": REVISION, "adapterAbi": "zoen-host-v1",
                  "profile": "rich-document-v1", "inputs": hashes, "wasiClocks": wasi,
                  "browser": browser, "compiled": False, "runtimeExecuted": False, "appAvailable": False}
        if foundation:
            result["archives"] = {Path(a.name).name: foundation.sha(Path(a.name)) for a in opened}
        receipt.write_text(json.dumps(result, indent=2) + "\n")
        return result
    finally:
        for archive in opened:
            archive.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", type=Path, required=True)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--wasi", action="store_true", help="Use the actual WASI system clocks")
    mode.add_argument("--browser", action="store_true", help="Prepare actual WordApp + pinned input foundation")
    parser.add_argument("--eframe-crate", type=Path)
    parser.add_argument("--egui-crate", type=Path)
    args = parser.parse_args()
    try:
        result = prepare(args.upstream, wasi=args.wasi, browser=args.browser,
                         eframe_crate=args.eframe_crate, egui_crate=args.egui_crate)
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, str(error) + "\n")
    print(json.dumps({"upstream": result["upstream"], "inputs": len(result["inputs"]),
                      "compiled": False, "runtimeExecuted": False}))


if __name__ == "__main__":
    main()
