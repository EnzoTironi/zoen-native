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
import sys

REVISION = "b8771d51fb4da60bd48c13cf313d6d6014dc5e66"
ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent


def git(directory, *args):
    return subprocess.check_output(["git", "-C", str(directory), *args], text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", type=Path, required=True)
    parser.add_argument("--browser", action="store_true", help="Include the managed WebGL/WASM shell")
    parser.add_argument("--eframe-crate", type=Path, help="Pinned eframe 0.36.2 archive for browser input")
    parser.add_argument("--egui-crate", type=Path, help="Pinned egui 0.36.2 archive for browser input")
    args = parser.parse_args()
    if args.browser and (sys.version_info < (3, 12) or not args.eframe_crate or not args.egui_crate):
        parser.error("--browser requires Python 3.12+ and both pinned --eframe-crate / --egui-crate archives")
    directory = args.upstream.resolve(strict=True)
    if git(directory, "rev-parse", "HEAD") != REVISION:
        raise SystemExit("Refusing unpinned GridCraft source")
    if git(directory, "status", "--porcelain", "--untracked-files=all"):
        raise SystemExit("Refusing dirty source: prepare a separate clean owned checkout")
    destination = directory / "crates/zoen-host"
    if destination.exists():
        raise SystemExit("Refusing an existing adapter directory")
    patches = [HERE / "patches/0001-managed-command-policy.patch",
               HERE / "patches/0002-web-without-filesystem.patch",
               HERE / "patches/0003-managed-share-intent.patch"]
    for patch in patches:
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
    if args.browser:
        recipe = HERE / "mobile-input"
        subprocess.run([sys.executable, str(recipe / "apply.py"), str(directory),
                        "--eframe-crate", str(args.eframe_crate.resolve(strict=True)),
                        "--egui-crate", str(args.egui_crate.resolve(strict=True))], check=True)
        for source in sorted(recipe.rglob("*")):
            if source.is_file() and "__pycache__" not in source.parts:
                inputs[str(source.relative_to(ROOT))] = hashlib.sha256(source.read_bytes()).hexdigest()
    # Refused pointer commands must preserve earlier undo history. Apply this
    # after mobile ownership, whose baseline is deliberately before 0004.
    undo = HERE / "patches/0004-refused-drag-keeps-undo.patch"
    subprocess.run(["git", "-C", str(directory), "apply", "--check", str(undo)], check=True)
    subprocess.run(["git", "-C", str(directory), "apply", str(undo)], check=True)
    patches.append(undo)
    # Compact editing depends on the existing browser/mobile TextEdit owner.
    # Headless preparation must retain its original unadapted UI sources.
    if args.browser:
        compact = HERE / "patches/0005-compact-retained-editor.patch"
        subprocess.run(["git", "-C", str(directory), "apply", "--check", str(compact)], check=True)
        subprocess.run(["git", "-C", str(directory), "apply", str(compact)], check=True)
        patches.append(compact)
        # Register after mobile's existing touch leaf so fresh preparation exactly
        # preserves the independently qualified test-source order in either profile.
        tests = destination / "src/ui_tests.rs"
        source = tests.read_text()
        if "mod compact;" in source:
            raise SystemExit("Refusing duplicate compact-layout test registration")
        tests.write_text(source + '\n#[path = "compact_ui_tests.rs"]\nmod compact;\n')
    # The source checker watches this prepared tree + these exact hashes. Cargo
    # must generate/update its lockfile genuinely, never using handwritten entries.
    for patch in patches:
        inputs[str(patch.relative_to(ROOT))] = hashlib.sha256(patch.read_bytes()).hexdigest()
    (directory / "zoen-gridcraft-source.json").write_text(json.dumps({
        "schema": 1, "upstream": REVISION, "adapterAbi": "zoen-host-v1",
        "inputs": inputs,
        "mobileInputArchives": ({key: hashlib.sha256(path.read_bytes()).hexdigest()
            for key, path in [("eframe-0.36.2", args.eframe_crate), ("egui-0.36.2", args.egui_crate)]}
            if args.browser else None),
        "compiled": False, "runtimeExecuted": False,
    }, indent=2) + "\n")
    print(json.dumps({"prepared": str(destination), "upstream": REVISION, "inputs": len(inputs)}))


if __name__ == "__main__":
    main()
