#!/usr/bin/env python3
"""Build native AMD64 images from an isolated, recorded Rust-only context."""
import argparse
import fcntl
import secrets
import hashlib
import json
import os
import pathlib
import shutil
import signal
import subprocess
import sys
import time

REPO = pathlib.Path(__file__).resolve().parents[2]
ROOT = REPO / '.dev/local'
DOCKER = ['docker', '--context', 'orbstack'] if sys.platform == 'darwin' else ['docker']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag', default='current')
    parser.add_argument('--proof', action='store_true', help='Also build the real native S3 contract probe')
    args = parser.parse_args()
    if not args.tag or any(c not in 'abcdefghijklmnopqrstuvwxyz0123456789-.' for c in args.tag):
        raise ValueError('Use a simple lowercase image tag')
    ROOT.mkdir(parents=True, exist_ok=True)
    run = ROOT / ('build-' + str(int(time.time())))
    run.mkdir(mode=0o700)
    (run / 'driver.json').write_text(json.dumps({'script_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(), 'argv': sys.argv}, indent=2) + '\n')
    context = run / 'context'
    context.mkdir()
    inventory = subprocess.check_output(['git', 'ls-files', '-co', '--exclude-standard', '-z'], cwd=REPO).decode().split('\0')
    deleted = set(subprocess.check_output(['git', 'ls-files', '--deleted', '-z'], cwd=REPO).decode().split('\0'))
    (run / 'checkout-deletions.json').write_text(json.dumps(sorted(deleted - {''}), indent=2) + '\n')
    rows = []
    for name in sorted(set(inventory)):
        if not name or name in deleted:
            continue
        allowed = name in ['Cargo.toml', 'Cargo.lock', '.dockerignore', 'infra/fly/relay.Dockerfile', 'infra/fly/fdb.Dockerfile', 'infra/local/proof.Dockerfile', 'infra/local/s3-contract.rs'] or name.startswith(('crates/', 'tools/uniffi-bindgen/', 'infra/fdb/'))
        if not allowed:
            continue
        source = REPO / name
        if not source.is_file() or source.is_symlink():
            raise ValueError('Build context requires regular tracked Rust/infra inputs: ' + name)
        destination = context / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
        rows.append({'path': name, 'sha256': hashlib.sha256(destination.read_bytes()).hexdigest(), 'bytes': destination.stat().st_size})
    (run / 'inputs.json').write_text(json.dumps(rows, indent=2) + '\n')
    lock = (ROOT / 'build.lock').open('a')
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    marker = ROOT / 'build-owner.json'
    daemon = subprocess.check_output(DOCKER + ['info', '--format', '{{.ID}}']).decode().strip()
    if marker.exists():
        if marker.is_symlink() or marker.stat().st_uid != os.getuid() or marker.stat().st_mode & 0o777 != 0o600:
            raise ValueError('Build owner marker must be a private owned regular file')
        owner = json.loads(marker.read_text())
        if owner['purpose'] != 'native-local-builder' or owner['daemon'] != daemon:
            raise ValueError('Build owner belongs to a different Docker engine')
        builder = owner['name']
        current = json.loads(subprocess.check_output(DOCKER + ['container', 'inspect', 'buildx_buildkit_' + builder + '0']))[0]
        if current['Id'] != owner['container'] or current['HostConfig']['Memory'] != 4 * 1024 ** 3 or current['HostConfig']['CpuQuota'] != 200000 or current['HostConfig']['CpuPeriod'] != 100000:
            raise ValueError('Refuse a foreign or differently bounded builder')
    else:
        builder = 'zoen-local-build-' + secrets.token_hex(6)
        subprocess.run(DOCKER + ['buildx', 'create', '--name', builder, '--driver', 'docker-container', '--driver-opt', 'memory=4g,cpu-period=100000,cpu-quota=200000'], check=True)
        subprocess.run(DOCKER + ['buildx', 'inspect', '--bootstrap', builder], check=True, stdout=subprocess.DEVNULL)
        current = json.loads(subprocess.check_output(DOCKER + ['container', 'inspect', 'buildx_buildkit_' + builder + '0']))[0]
        marker.write_text(json.dumps({'purpose': 'native-local-builder', 'name': builder, 'daemon': daemon, 'container': current['Id']}, indent=2) + '\n')
        marker.chmod(0o600)
    try:
        for kind in ['relay', 'fdb'] + (['s3-proof'] if args.proof else []):
            tag = 'zoen-local-' + kind + ':' + args.tag
            dockerfile = 'infra/local/proof.Dockerfile' if kind == 's3-proof' else 'infra/fly/' + kind + '.Dockerfile'
            command = DOCKER + ['buildx', 'build', '--builder', builder, '--platform', 'linux/amd64', '--progress', 'plain', '--load', '-f', str(context / dockerfile), '-t', tag, str(context)]
            log = run / (kind + '.log')
            start = time.monotonic()
            reason = None
            with log.open('w') as stream:
                child = subprocess.Popen(command, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
                try:
                    while child.poll() is None:
                        if shutil.disk_usage(ROOT).free < 5 * 1024 ** 3:
                            reason = 'Less than 5 GiB disk headroom'
                        elif log.stat().st_size > 8 * 1024 ** 2:
                            reason = 'Build log exceeded 8 MiB'
                        elif time.monotonic() - start > 1200:
                            reason = 'Build exceeded 20 minutes'
                        if reason:
                            os.killpg(child.pid, signal.SIGTERM)
                            break
                        time.sleep(2)
                finally:
                    if child.poll() is None:
                        os.killpg(child.pid, signal.SIGTERM)
                        try:
                            child.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            os.killpg(child.pid, signal.SIGKILL)
                            child.wait()
            result = {'command': command, 'exit_code': child.returncode, 'guard_failure': reason, 'elapsed_seconds': time.monotonic() - start, 'log_sha256': hashlib.sha256(log.read_bytes()).hexdigest()}
            if child.returncode == 0 and reason is None:
                image = json.loads(subprocess.check_output(DOCKER + ['image', 'inspect', tag]))[0]
                result['image'] = {'tag': tag, 'id': image['Id'], 'architecture': image['Architecture'], 'bytes': image['Size']}
            (run / (kind + '.json')).write_text(json.dumps(result, indent=2) + '\n')
            if child.returncode or reason:
                raise RuntimeError('Native image build failed; inspect ' + str(log))
            print('Built ' + tag + '; evidence ' + str(run), flush=True)
    finally:
        # Retain the owned compiler cache; release its running process/memory.
        subprocess.run(DOCKER + ['buildx', 'stop', builder], check=False)
        lock.close()


if __name__ == '__main__':
    main()
