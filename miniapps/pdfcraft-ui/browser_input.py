"""Add the qualified input foundation while retaining PdfCraft's renderer patches."""
from pathlib import Path
import importlib.util
import re
import subprocess
import tomllib

HERE = Path(__file__).resolve().parent
WORD = HERE.parent / "wordcraft-ui/browser_input.py"
spec = importlib.util.spec_from_file_location("pdfcraft_shared_input", WORD)
common = importlib.util.module_from_spec(spec)
spec.loader.exec_module(common)
ORIGINAL_PATCHES = {
    "hayro-interpret", "hayro", "hayro-syntax", "hayro-cmap", "hayro-postscript",
    "hayro-jbig2", "lopdf", "winit", "egui-wgpu", "vello_common",
}


def inputs():
    # Word's editor-specific canvas changes are not applicable to PdfCraft.
    return [Path(__file__).resolve(), *[path for path in common.inputs()
            if path.name != "0004-word-canvas-input.patch"]]


def archives(eframe, egui):
    return common.archives(eframe, egui)


def apply(directory, opened):
    vendor = directory / "vendor"
    if vendor.is_symlink() or not vendor.is_dir():
        raise ValueError("Missing pinned PdfCraft vendor tree")
    cargo = directory / "Cargo.toml"
    text = cargo.read_text()
    patches = tomllib.loads(text).get("patch", {}).get("crates-io", {})
    if set(patches) != ORIGINAL_PATCHES:
        raise ValueError("Changed upstream renderer/platform patch table")
    if any((vendor / name).exists() or (vendor / name).is_symlink()
           for name in ("eframe-0.36.2", "egui-0.36.2")):
        raise ValueError("Refusing existing shared input output")
    section = re.search(r"(?m)^\[patch\.crates-io\]\n.*?(?=^\[|\Z)", text, re.S)
    if not section:
        raise ValueError("Missing crate patch section")
    for archive in opened:
        archive.extractall(vendor, filter="data")
    for name in common.NAMES:
        path = common.SHARED / "patches" / name
        subprocess.run(["git", "apply", "--check", str(path)], cwd=directory, check=True)
        subprocess.run(["git", "apply", str(path)], cwd=directory, check=True)
    insertion = '\neframe = { path = "vendor/eframe-0.36.2" }\negui = { path = "vendor/egui-0.36.2" }\n'
    cargo.write_text(text[:section.end()] + insertion + text[section.end():])
