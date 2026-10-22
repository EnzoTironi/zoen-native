#!/usr/bin/env python3
"""Build pinned, managed DeckCraft code assets in a fresh private directory. No publication."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tarfile
import time

from dependency_experiment import EXPERIMENT_CHAINS, EXPERIMENT_NAMES, load_experiment, stage_experiment

ROOT = Path(__file__).resolve().parent

def sha(data):
    return hashlib.sha256(data).hexdigest()

def checked_file(path, digest):
    data = path.read_bytes()
    if sha(data) != digest:
        raise ValueError('Input hash mismatch: ' + str(path))
    return data

def run(command, cwd, env=None, timeout=30):
    return subprocess.check_output(command, cwd=cwd, env=env, timeout=timeout, stderr=subprocess.STDOUT).decode()

def prepare(upstream, fonts, work, metadata, experiments=()):
    # Archive the pinned Git object, never the caller's working tree or local changes.
    archive = subprocess.check_output(['git', '-C', str(upstream), 'archive', '--format=tar', metadata['revision']], timeout=30)
    source = work / 'source'
    source.mkdir()
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        for member in tar.getmembers():
            target = (source / member.name).resolve()
            if source not in target.parents or not (member.isfile() or member.isdir()):
                raise ValueError('Unsafe archive member: ' + member.name)
        tar.extractall(source)
    checked_file(source / 'Cargo.lock', metadata['originalCargoLockSha256'])
    for patch in metadata['patches']:
        path = ROOT / patch['path']
        checked_file(path, patch['sha256'])
        run(['git', 'apply', '--check', str(path)], source)
        run(['git', 'apply', str(path)], source)
    checked_file(source / 'Cargo.lock', metadata['managedCargoLockSha256'])
    proofs = []
    for experiment, archive in experiments:
        def apply_dependency_patch(target, patch):
            run(['git', 'apply', '--check', str(patch)], target)
            run(['git', 'apply', str(patch)], target)
        proofs.append(stage_experiment(source, experiment, archive, ROOT, apply_dependency_patch))
    if len(proofs) == 1:
        (work / 'dependency-experiment.json').write_text(json.dumps(proofs[0], indent=2) + '\n')
    elif proofs:
        (work / 'dependency-experiments.json').write_text(json.dumps(proofs, indent=2) + '\n')
    index = source / 'apps/deckcraft-web/index.html'
    original = index.read_text()
    marker = 'data-bin="deckcraft-web"'
    if original.count(marker) != 1:
        raise ValueError('Unexpected Trunk entry point')
    # Trunk's public Rust-asset feature option, not a shared-source mutation.
    index.write_text(original.replace(marker, marker + ' data-cargo-features="zoen-host"'))
    font_metadata = json.loads((ROOT / 'font-input/upstream.json').read_text())
    destination = work / 'fonts'
    for item in font_metadata['fonts'] + font_metadata['licenseFiles']:
        data = checked_file(fonts / item['path'], item['sha256'])
        if len(data) != item['bytes']:
            raise ValueError('Font length mismatch')
        path = destination / item['path']
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    for name in ['manifest.txt', 'latin-manifest.txt']:
        shutil.copyfile(ROOT / 'font-input' / name, destination / 'fonts' / name)
    return source, destination

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--upstream', required=True, type=Path, help='Local Git checkout containing the exact pinned upstream object')
    parser.add_argument('--fonts', required=True, type=Path, help='Local craft-fonts directory; only exact verified subset is read')
    parser.add_argument('--work', required=True, type=Path, help='New private output directory; never a Zoen checkout')
    parser.add_argument('--asset-base-url', '--base-url', dest='base_url', help='Explicit anonymous HTTPS code-asset base URL ending /; no default deployment')
    parser.add_argument('--trunk', default='trunk')
    parser.add_argument('--wasm-opt', default='wasm-opt')
    parser.add_argument('--node', default='node')
    parser.add_argument('--cache-target', type=Path, help='Optional existing inactive Cargo target, copied read-only into the private target')
    parser.add_argument('--deadline', type=int, default=1800)
    parser.add_argument('--prepare-only', action='store_true')
    parser.add_argument('--experiment', choices=EXPERIMENT_NAMES, action='append', default=[], help='Explicit ordered dependency experiment; repeat only for a reviewed chain, default off')
    parser.add_argument('--experiment-crate', type=Path, action='append', default=[], help='Matching ordered exact local .crate archive; never downloads or edits Cargo registry source')
    args = parser.parse_args()
    metadata = json.loads((ROOT / 'upstream.json').read_text())
    if len(args.experiment) != len(args.experiment_crate):
        raise ValueError('Experiment and exact local crate input must be supplied together')
    if args.experiment and tuple(args.experiment) not in EXPERIMENT_CHAINS:
        raise ValueError('Unreviewed dependency experiment chain')
    experiments = []
    stage_base = metadata
    for name, archive in zip(args.experiment, args.experiment_crate):
        experiment = load_experiment(ROOT, name, stage_base)
        experiments.append((experiment, archive.resolve(strict=True)))
        stage_base = dict(metadata, managedCargoLockSha256=experiment['managedCargoLockSha256'])
    if not 1 <= args.deadline <= 3600 or not args.work.is_absolute():
        raise ValueError('Use an absolute private directory and a bounded deadline <=3600s')
    work = args.work.resolve()
    if work == ROOT or ROOT in work.parents or (ROOT.parents[1] in work.parents):
        raise ValueError('Build outputs must live outside the Zoen source checkout')
    if args.base_url and not args.base_url.endswith('/'):
        raise ValueError('An explicit code-asset base URL must end in /; build does not publish it')
    work.mkdir(exist_ok=False, parents=True)
    source, fonts = prepare(args.upstream.resolve(), args.fonts.resolve(), work, metadata, experiments)
    receipt = {'revision': metadata['revision'], 'patches': metadata['patches'], 'prepared': True, 'advertised': False,
               'jobs': 1, 'binaryenCores': 1, 'deadlineSeconds': args.deadline}
    if len(experiments) == 1:
        receipt['dependencyExperiment'] = json.loads((work / 'dependency-experiment.json').read_text())
    elif experiments:
        receipt['dependencyExperiments'] = json.loads((work / 'dependency-experiments.json').read_text())
    (work / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    if args.prepare_only:
        (work / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        return
    if args.cache_target:
        cache = args.cache_target.resolve(strict=True)
        if not cache.is_dir() or cache == work or work in cache.parents:
            raise ValueError('Cache must be a separate inactive target directory')
        if sys.platform == 'darwin':
            run(['cp', '-cR', str(cache), str(work / 'target')], work, timeout=120)
        else:
            shutil.copytree(cache, work / 'target')
        receipt['cacheCopiedFrom'] = str(cache)
    tools = {name: run(command + ['--version'], work).strip() for name, command in {
        'rustc': ['rustc'], 'cargo': ['cargo'], 'trunk': [args.trunk], 'wasmBindgen': ['wasm-bindgen'], 'binaryen': [args.wasm_opt]}.items()}
    for name, version in metadata['toolVersions'].items():
        if version not in tools[name]:
            raise ValueError('Unexpected ' + name + ' version: ' + tools[name])
    env = dict(os.environ, CARGO_BUILD_JOBS='1', CARGO_INCREMENTAL='0', CARGO_PROFILE_RELEASE_DEBUG='0',
               CARGO_TARGET_DIR=str(work / 'target'), CRAFT_FONTS_DIR=str(fonts), CRAFT_FONTS_REQUIRED='1',
               DECKCRAFT_BUILD_SHA=metadata['revision'], **metadata['reproducibleEnvironment'],
               TRUNK_TOOLS_WASM_BINDGEN=metadata['toolVersions']['wasmBindgen'], TRUNK_TOOLS_WASM_OPT=metadata['toolVersions']['binaryen'],
               TRUNK_SKIP_VERSION_CHECK='true', BINARYEN_CORES='1')
    env['PATH'] = str(Path(shutil.which(args.wasm_opt)).parent) + os.pathsep + env.get('PATH', '')
    command = [str(Path(shutil.which(args.trunk))), 'build', '--release', '--locked=true', '--offline=true', '--skip-version-check', '--dist', str(work / 'dist')]
    started = time.monotonic()
    timed_out = False
    log = work / 'build.log'
    with log.open('wb') as output:
        process = subprocess.Popen(command, cwd=source / 'apps/deckcraft-web', env=env,
                                   stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = process.wait(timeout=args.deadline)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(process.pid, signal.SIGTERM)
            try:
                code = process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                code = process.wait()
    receipt.update(exitCode=code, timedOut=timed_out, elapsedSeconds=round(time.monotonic() - started, 3),
                   toolVersions=tools, command=command, logSha256=sha(log.read_bytes()))
    (work / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    if code != 0 or timed_out:
        raise SystemExit('Managed release failed; preserve receipt/build.log')
    package = work / ('runtime-package.json' if args.base_url else 'assets.json')
    run([args.node, str(ROOT / 'audit-assets.mjs'), str(work / 'dist'), args.base_url or '-',
         metadata['revision'], str(package)], ROOT, timeout=60)
    shutil.copytree(ROOT / 'licenses', work / 'licenses')
    shutil.copyfile(ROOT / 'NOTICE.md', work / 'NOTICE.md')
    # HTML from Trunk contains standalone startup; do not distribute it as the MCP App.
    receipt['assetMetadataSha256'] = sha(package.read_bytes())
    receipt['mcpResourceReady'] = False
    (work / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')

if __name__ == '__main__':
    main()
