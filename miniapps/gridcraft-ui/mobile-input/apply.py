#!/usr/bin/env python3
"""Apply pinned Grid mobile input recipe to the existing prepared Grid source.

Run after existing 0001-0003, before 0004; then run normal offline Cargo
metadata to produce the lockfile and normal compiler/browser qualification.
This script does not build, change registry caches, or publish runtime assets.
"""
from pathlib import Path
import argparse
import hashlib
import json
import subprocess
import tarfile
import sys


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    if sys.version_info < (3, 12):
        raise RuntimeError("Python 3.12 or later is required (bundled runtime is supported)")
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('prepared', type=Path)
    cli.add_argument('--eframe-crate', required=True, type=Path)
    cli.add_argument('--egui-crate', required=True, type=Path)
    args = cli.parse_args()
    recipe = Path(__file__).resolve().parent
    manifest = json.loads((recipe / 'patches.json').read_text())
    prepared = args.prepared.resolve(strict=True)
    if not prepared.is_dir() or args.prepared.is_symlink():
        raise ValueError('prepared source must be a real directory')
    vendor = prepared / 'vendor'
    if vendor.exists() or vendor.is_symlink():
        raise ValueError('vendor already exists; use a fresh prepared source')
    for name, expected in manifest['baselineFiles'].items():
        path = prepared / name
        if path.is_symlink() or digest(path) != expected:
            raise ValueError(f'prepared baseline mismatch: {name}')
    patches = []
    for entry in manifest['patches']:
        path = recipe / entry['path']
        if path.is_symlink() or path.stat().st_size != entry['bytes'] or digest(path) != entry['sha256']:
            raise ValueError(f'recipe patch mismatch: {path.name}')
        patches.append(path)
    test = recipe / manifest['testLeaf']['path']
    if test.is_symlink() or digest(test) != manifest['testLeaf']['sha256']:
        raise ValueError('test leaf mismatch')
    target = prepared / 'crates/zoen-host/src/touch_ui_tests.rs'
    parent = target.with_name('ui_tests.rs')
    declaration = '\n#[path = "touch_ui_tests.rs"]\nmod touch;\n'
    if target.exists() or target.is_symlink() or parent.is_symlink() or not parent.is_file():
        raise ValueError('copy the current host sources before applying this recipe once')
    if not parent.resolve().is_relative_to(prepared):
        raise ValueError('host source leaves the prepared tree')
    parent_source = parent.read_text()
    if 'mod touch;' in parent_source:
        raise ValueError('touch test registration already exists')
    archives = [('eframe-0.36.2', args.eframe_crate), ('egui-0.36.2', args.egui_crate)]
    opened = []
    try:
        for name, path in archives:
            if path.is_symlink() or digest(path) != manifest['archives'][name]:
                raise ValueError(f'pinned archive mismatch: {name}')
            archive = tarfile.open(path, 'r:gz')
            opened.append(archive)
            total = 0
            for member in archive.getmembers():
                relative = Path(member.name)
                if relative.is_absolute() or '..' in relative.parts or relative.parts[0] != name:
                    raise ValueError('archive path is not in its exact crate root')
                if not (member.isfile() or member.isdir()):
                    raise ValueError('archive must contain only regular files/directories')
                total += member.size
                if total > 32 * 1024 * 1024:
                    raise ValueError('archive exceeds extraction bound')
        # All closed inputs were verified before the first source mutation.
        vendor.mkdir()
        for archive in opened:
            archive.extractall(vendor, filter='data')
        for patch in patches:
            subprocess.run(['git', 'apply', '--check', str(patch)], cwd=prepared, check=True)
            subprocess.run(['git', 'apply', str(patch)], cwd=prepared, check=True)
        target.write_bytes(test.read_bytes())
        parent.write_text(parent_source + declaration)
    finally:
        for archive in opened:
            archive.close()
    print(json.dumps({'status': 'APPLIED_NOT_COMPILED', 'source': manifest['source'], 'prepared': str(prepared)}))


if __name__ == '__main__':
    main()
