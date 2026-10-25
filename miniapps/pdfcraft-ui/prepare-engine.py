#!/usr/bin/env python3
"""Prepare an exact owned PdfCraft checkout without compiling or activating an App."""
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


def git(directory, *arguments):
    return subprocess.check_output(["git", "-C", str(directory), *arguments], text=True).strip()


def browser_recipe():
    spec = importlib.util.spec_from_file_location("pdf_browser_input", HERE / "browser_input.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def prepare(directory, managed=False, browser=False, eframe_crate=None, egui_crate=None):
    if browser and not managed:
        raise ValueError("Browser preparation requires the managed adapter")
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
    if target.exists() or receipt.exists() or (directory / "crates/zoen-web").exists():
        raise ValueError("Refusing existing prepared output")
    inputs = [Path(__file__).resolve(), HERE / "upstream.json", *files]
    copies = [(path, target / path.relative_to(source)) for path in files]
    structure = ROOT / "crates/miniapps/src/pdfcraft/structure.rs"
    if managed:
        if structure.is_symlink() or not structure.is_file():
            raise ValueError("Missing shared structural source")
        for name, path in [("structure.rs", structure), ("action.rs", structure.with_name("action.rs"))]:
            inputs.append(path)
            copies.append((path, target / "src" / name))
    foundation = browser_recipe() if browser else None
    patches = []
    if browser:
        inputs.extend(foundation.inputs())
        patches = [HERE / "patches/0001-managed-session.patch",
                   HERE / "patches/0002-render-target-fields.patch",
                   HERE / "patches/0003-ui-target-paths.patch",
                   HERE / "patches/0004-paragraph-touch-release.patch",
                   HERE / "patches/0005-retained-canonical-undo.patch"]
        for path in sorted((HERE / "browser").rglob("*")):
            if path.is_symlink():
                raise ValueError("Refusing symlinked browser inputs")
            if path.is_file():
                inputs.append(path)
                copies.append((path, directory / "crates/zoen-web" / path.relative_to(HERE / "browser")))
        layout = HERE.parent / "gridcraft-ui/browser/src/layout.rs"
        inputs.extend([layout, *patches])
        copies.append((layout, directory / "crates/zoen-web/src/layout.rs"))
    if any(path.is_symlink() or not path.is_file() for path in inputs):
        raise ValueError("Refusing non-regular recipe input")
    result = {"schema": 1, "upstream": CONFIG["revision"],
              "upstreamTree": git(directory, "rev-parse", "HEAD^{tree}"),
              "mode": "managed-binary-browser" if browser else ("managed-binary-native" if managed else "actual-engine-and-editor-inspection"),
              "inputs": {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs},
              "sourceManifests": CONFIG["sourceManifests"],
              "profileChosen": False, "compiled": False, "runtimeExecuted": False, "appAvailable": False}
    # Validate the exact common input archives before touching the owned source.
    # PdfCraft's original renderer/platform vendor patches are preserved.
    opened = foundation.archives(eframe_crate, egui_crate) if foundation else []
    try:
        for patch in patches:
            subprocess.run(["git", "apply", "--check", str(patch)], cwd=directory, check=True)
        for path, destination in copies:
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, destination)
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
    parser.add_argument("--managed", action="store_true")
    parser.add_argument("--browser", action="store_true")
    parser.add_argument("--eframe-crate", type=Path)
    parser.add_argument("--egui-crate", type=Path)
    args = parser.parse_args()
    try:
        result = prepare(args.upstream, args.managed, args.browser, args.eframe_crate, args.egui_crate)
    except (ValueError, OSError, KeyError, subprocess.SubprocessError) as error:
        parser.exit(1, str(error) + "\n")
    print(json.dumps({"upstream": result["upstream"], "inputs": len(result["inputs"]),
                      "compiled": False, "runtimeExecuted": False, "appAvailable": False}))


if __name__ == "__main__":
    main()
