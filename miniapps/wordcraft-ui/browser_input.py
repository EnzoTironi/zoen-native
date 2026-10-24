"""Pinned common egui/AppRunner input foundation for the actual Word canvas.

This uses the qualified shared Grid input patches, never its editor or state. No
registry cache is modified; the caller owns a newly prepared upstream checkout.
"""
from pathlib import Path
import hashlib
import json
import subprocess
import sys
import tarfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
SHARED = HERE.parent / 'gridcraft-ui/mobile-input'
NAMES = ('eframe-current-owner.patch', 'eframe-primary-contact.patch',
         'eframe-text-agent.patch', 'egui-touch-input.patch')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inputs():
    manifest = SHARED / 'patches.json'
    data = json.loads(manifest.read_text())
    result = [Path(__file__).resolve(), manifest]
    for name in NAMES:
        entry = next(p for p in data['patches'] if p['path'] == 'patches/' + name)
        path = SHARED / entry['path']
        if path.is_symlink() or path.stat().st_size != entry['bytes'] or sha(path) != entry['sha256']:
            raise ValueError('Shared input patch mismatch: ' + name)
        result.append(path)
    return result


def archives(eframe, egui):
    if sys.version_info < (3, 12):
        raise ValueError('Use Python 3.12+ for bounded archive extraction')
    manifest = json.loads((SHARED / 'patches.json').read_text())
    opened = []
    try:
        for name, path in [('eframe-0.36.2', eframe), ('egui-0.36.2', egui)]:
            if path is None or path.is_symlink() or sha(path) != manifest['archives'][name]:
                raise ValueError('Pinned archive mismatch: ' + name)
            archive = tarfile.open(path, 'r:gz')
            opened.append(archive)
            total = 0
            for member in archive.getmembers():
                rel = Path(member.name)
                if rel.is_absolute() or '..' in rel.parts or not rel.parts or rel.parts[0] != name:
                    raise ValueError('Archive path leaves its exact crate root')
                if not (member.isfile() or member.isdir()):
                    raise ValueError('Archive must contain only regular files/directories')
                total += member.size
                if total > 32 * 1024 * 1024:
                    raise ValueError('Archive exceeds extraction bound')
        return opened
    except BaseException:
        for archive in opened:
            archive.close()
        raise


def apply(directory, opened):
    vendor = directory / 'vendor'
    if vendor.exists() or vendor.is_symlink():
        raise ValueError('Refusing existing vendor directory')
    cargo = directory / 'Cargo.toml'
    text = cargo.read_text()
    section = '[patch.crates-io]\n'
    if text.count(section) != 1 or 'eframe = { path = "vendor/' in text or 'egui = { path = "vendor/' in text:
        raise ValueError('Unexpected upstream patch section')
    vendor.mkdir()
    for archive in opened:
        archive.extractall(vendor, filter='data')
    for path in inputs()[2:]:
        subprocess.run(['git', 'apply', '--check', str(path)], cwd=directory, check=True)
        subprocess.run(['git', 'apply', str(path)], cwd=directory, check=True)
    cargo.write_text(text.replace(section, section +
        'eframe = { path = "vendor/eframe-0.36.2" }\n'
        'egui = { path = "vendor/egui-0.36.2" }\n'))
