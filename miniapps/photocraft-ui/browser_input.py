"""Reuse the exact qualified shared input foundation; no editor state is shared."""
from pathlib import Path
import importlib.util
import subprocess
HERE = Path(__file__).resolve().parent
WORD = HERE.parent / "wordcraft-ui/browser_input.py"
spec = importlib.util.spec_from_file_location("photocraft_shared_input", WORD)
common = importlib.util.module_from_spec(spec)
spec.loader.exec_module(common)


def inputs():
    # Word's archive validation and pinned common paths are reused. Its engine
    # canvas patch is not applicable to PhotoCraft's ordinary TextEdit widgets.
    return [Path(__file__).resolve(), *[p for p in common.inputs()
        if p.name != "0004-word-canvas-input.patch"]]


def archives(eframe, egui):
    return common.archives(eframe, egui)


def apply(directory, opened):
    vendor = directory / "vendor"
    if vendor.exists() or vendor.is_symlink():
        raise ValueError("Refusing existing vendor tree")
    cargo = directory / "Cargo.toml"
    text = cargo.read_text()
    if "[patch.crates-io]" in text:
        raise ValueError("Unexpected upstream crate patch section")
    vendor.mkdir()
    for archive in opened:
        archive.extractall(vendor, filter="data")
    for name in common.NAMES:
        path = common.SHARED / "patches" / name
        subprocess.run(["git", "apply", "--check", str(path)], cwd=directory, check=True)
        subprocess.run(["git", "apply", str(path)], cwd=directory, check=True)
    cargo.write_text(text + '\n[patch.crates-io]\neframe = { path = "vendor/eframe-0.36.2" }\negui = { path = "vendor/egui-0.36.2" }\n')
