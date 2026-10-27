#!/usr/bin/env python3
"""Bound #320 archive recovery; sdkmanager still verifies and installs pinned packages."""
import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

# Official ownership/lifecycle: BasicInstaller.doPrepare and
# AbstractPackageOperation.readOrCreateInstallProperties/prepare at:
# https://android.googlesource.com/platform/tools/base/+/9e7b37ce603f579b19492556c1f4301b7a09aa84/repository/src/main/java/com/android/repository/impl/installer/
# Failed preparation retains its .installData-linked stage. Only that NEW stage
# is reset here. Never remove a package, global cache, or prepared installation.
PACKAGES = ('platforms;android-37.0', 'build-tools;37.0.0', 'ndk;27.1.12297006')
ARCHIVE_FAILURE = re.compile(
    r'^Warning: An error occurred while preparing SDK package [^\n]+: '
    r'(?:Error on ZipFile unknown archive|zip END header not found|'
    r'Unexpected end of ZLIB input stream)\.?$', re.MULTILINE)
ANSI = re.compile(r'\x1b\[[0-?]*[ -/]*[@-~]')
ATTEMPTS = 3


def stage_inventory(sdk):
    directory = sdk / '.temp'
    if directory.is_symlink():
        raise ValueError('SDK staging root is a symlink')
    return set(directory.iterdir()) if directory.exists() else set()


def clear_owned_stage(sdk, package, before, owned):
    target = sdk.joinpath(*package.split(';'))
    for path in (target, target.parent, target / '.installer'):
        if path.is_symlink():
            raise ValueError('Package staging metadata uses a symlink')
    if any((target / name).exists() or (target / name).is_symlink()
           for name in ('package.xml', 'source.properties')):
        raise ValueError('Refusing recovery against an installed package')
    metadata = target / '.installer' / '.installData'
    if metadata.is_symlink() or not metadata.is_file():
        raise ValueError('No regular installer ownership metadata')
    properties = {}
    for line in metadata.read_text().splitlines():
        if not line or line.startswith(('#', '!')):
            continue
        key, separator, value = line.partition('=')
        if not separator or key in properties or '\\' in line:
            raise ValueError('Unrecognized installer properties')
        properties[key] = value
    if properties.get('class') != 'com.android.repository.impl.installer.BasicInstaller':
        raise ValueError('Unknown installer class')
    stage = Path(properties.get('path', ''))
    if (not stage.is_absolute() or stage.parent != sdk / '.temp'
            or not re.fullmatch(r'PackageOperation(?:0[1-9]|[1-9][0-9])', stage.name)
            or stage.is_symlink() or stage.resolve() != stage or not stage.is_dir()):
        raise ValueError('Unrecognized installer staging path')
    if stage in before and stage not in owned:
        raise ValueError('Staging predates this installer invocation')
    if (stage / '.prepareComplete').exists() or (stage / '.prepareComplete').is_symlink():
        raise ValueError('Refusing to clear prepared installation')
    # rmtree does not follow interior symlinks. SDK package files and metadata
    # remain untouched; the same sdkmanager operation downloads/verifies again.
    shutil.rmtree(stage)
    stage.mkdir()
    owned.add(stage)
    print(f'Retrying archive preparation; reset owned stage {stage}', flush=True)


def archive_failure_only(output):
    # Mixed license/configuration/other errors must not borrow a corrupt-archive
    # warning as permission to retry. Keep the recognition intentionally narrow.
    diagnostics = [line.strip() for line in output.splitlines()
                   if re.match(r'^(?:Warning:|Error:|Exception\b|[\w.$]+Exception:)', line.strip())]
    return bool(diagnostics) and all(ARCHIVE_FAILURE.fullmatch(line) for line in diagnostics)


def install(sdk, logs, manager='sdkmanager', packages=PACKAGES):
    sdk = Path(sdk).resolve(strict=True)
    logs = Path(logs)
    logs.mkdir(parents=True, exist_ok=True)
    for package in packages:
        owned = set()
        for attempt in range(1, ATTEMPTS + 1):
            before = stage_inventory(sdk)
            path = logs / (package.replace(';', '-') + f'-attempt-{attempt}.log')
            # Exclusive creation preserves previous attempts/runs rather than
            # overwriting the failure. Bytes below are raw combined stdout/stderr.
            with path.open('xb') as log:
                process = subprocess.Popen(
                    [manager, '--sdk_root=' + str(sdk), package],
                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
                for line in iter(process.stdout.readline, b''):
                    log.write(line)
                    log.flush()
                    sys.stdout.buffer.write(line)
                    sys.stdout.buffer.flush()
                code = process.wait()
            if code == 0:
                break
            output = ANSI.sub('', path.read_text(errors='replace')).replace('\r', '\n')
            if attempt == ATTEMPTS or not archive_failure_only(output):
                return code if code > 0 else 1
            clear_owned_stage(sdk, package, before, owned)
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sdk-root', default=os.environ.get('ANDROID_HOME') or os.environ.get('ANDROID_SDK_ROOT'))
    parser.add_argument('--logs', required=True, type=Path)
    args = parser.parse_args()
    if not args.sdk_root:
        parser.error('ANDROID_HOME/ANDROID_SDK_ROOT or --sdk-root required')
    try:
        return install(args.sdk_root, args.logs)
    except (OSError, ValueError) as error:
        print(f'Android SDK installation stopped: {error}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
