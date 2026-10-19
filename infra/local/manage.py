#!/usr/bin/env python3
"""Manage only this instance's native server, private dependencies and persistent volumes."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import secrets
import shutil
import socket
import subprocess
import sys
import time
import urllib.request

REPO = pathlib.Path(__file__).resolve().parents[2]
ROOT = REPO / '.dev' / 'local'
DOCKER = ['docker', '--context', 'orbstack'] if sys.platform == 'darwin' else ['docker']
PURPOSE = 'native-local-environment'
PRIVATE_FILES = ('postgres.env', 'storage.env', 'relay.env')
IMAGE_REFS = {
    'relay': 'zoen-local-relay:current',
    'fdb': 'zoen-local-fdb:7.3.71',
    'postgres': 'postgres:17-bookworm',
    'storage': 'chrislusf/seaweedfs:4.48@sha256:4e61d15fd35994cb1e43e1e553dff106794841fd9a99ade2fc8c8bfce4d7872d',
    'archive': 'debian:bookworm-slim',
    'bus': 'nats:2.15.0-alpine',
}


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def sha256(path):
    h = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')
    path.chmod(0o600)


def command(args, capture=True, timeout=120):
    result = subprocess.run(args, check=False, capture_output=capture, text=True, timeout=timeout)
    if result.returncode:
        raise RuntimeError(f'{args[0]} {args[1:3]} failed ({result.returncode}): {(result.stderr or "")[-2000:]}')
    return result.stdout if capture else ''


def inspect(kind, name):
    result = subprocess.run(DOCKER + [kind, 'inspect', name], capture_output=True, text=True, timeout=10)
    try:
        values = json.loads(result.stdout)
    except json.JSONDecodeError:
        raise RuntimeError(f'Docker {kind} inspect unavailable; refuse lifecycle action') from None
    if result.returncode and values != []:
        raise RuntimeError(f'Docker {kind} inspect failed; refuse lifecycle action')
    return values[0] if values else None


def validate_name(name):
    require(bool(name) and len(name) <= 32 and all(c in 'abcdefghijklmnopqrstuvwxyz0123456789-' for c in name), 'Instance names use lowercase letters, numbers and hyphens')


def instance(name):
    validate_name(name)
    path = ROOT / name
    require(not path.is_symlink(), 'No symlink state directories')
    owner = json.loads((path / 'owner.json').read_text())
    require(owner['schema'] == 1 and owner['purpose'] == PURPOSE and owner['name'] == name and owner['root'] == str(path), 'Invalid instance identity')
    require(owner['project'] == 'zoen-local-' + name and len(owner['nonce']) == 64, 'Invalid resource owner')
    require(path.stat().st_uid == os.getuid() and path.stat().st_mode & 0o777 == 0o700, 'Private state must be owned and mode 0700')
    for file in PRIVATE_FILES:
        source = path / file
        require(source.is_file() and not source.is_symlink() and source.stat().st_uid == os.getuid() and source.stat().st_mode & 0o777 == 0o600, 'Private env files must be owned regular mode 0600')
    return path, owner


def compose(path, owner):
    result = DOCKER + ['compose', '--project-name', owner['project'], '--env-file', str(path / 'compose.env'), '-f', str(REPO / 'infra/local/compose.yaml')]
    if owner['nodes'] == 2:
        result += ['-f', str(REPO / 'infra/local/compose.cluster.yaml')]
    return result


def owned(path, owner):
    rows = command(DOCKER + ['ps', '-a', '--filter', 'label=com.docker.compose.project=' + owner['project'], '--format', '{{.ID}}']).splitlines()
    for identifier in rows:
        value = json.loads(command(DOCKER + ['inspect', identifier]))[0]
        labels = value['Config'].get('Labels', {})
        require(labels.get('app') == 'zoen.local' and labels.get('owner') == owner['nonce'], 'Foreign container; refuse lifecycle action')
    for kind, suffixes in [('volume', volume_names(owner)), ('network', ['private', 'edge'])]:
        for suffix in suffixes:
            value = inspect(kind, owner['project'] + '_' + suffix)
            if value:
                labels = value.get('Labels', {})
                require(labels.get('app') == 'zoen.local' and labels.get('owner') == owner['nonce'], 'Foreign persistent resource; refuse')
    command(compose(path, owner) + ['config', '--quiet'])  # Never print expanded secrets.


def init(name, port, nodes, relay_image, fdb_image):
    validate_name(name)
    require(port not in [8790, 5547, 8089] and 1024 <= port <= 65534, 'Choose an unprotected unprivileged port')
    require(nodes in [1, 2] and (nodes == 1 or port + 1 not in [8790, 5547, 8089]), 'Invalid node count or protected second port')
    ROOT.mkdir(parents=True, exist_ok=True)
    path = ROOT / name
    require(not path.exists() and not path.is_symlink(), 'Existing instance is never replaced by init')
    path.mkdir(mode=0o700)
    refs = {**IMAGE_REFS, 'relay': relay_image, 'fdb': fdb_image}
    owner = {'schema': 1, 'purpose': PURPOSE, 'name': name, 'root': str(path), 'project': 'zoen-local-' + name, 'nonce': secrets.token_hex(32), 'port': port, 'nodes': nodes, 'image_refs': refs, 'images': {}}
    write_json(path / 'owner.json', owner)
    password, access, secret = secrets.token_hex(24), secrets.token_hex(10), secrets.token_hex(32)
    envs = {
        'postgres.env': {'POSTGRES_USER': 'zoen', 'POSTGRES_DB': 'zoen_relay', 'POSTGRES_PASSWORD': password, 'PGDATA': '/var/lib/postgresql/data/pgdata'},
        'storage.env': {'AWS_ACCESS_KEY_ID': access, 'AWS_SECRET_ACCESS_KEY': secret, 'S3_BUCKET': 'encrypted'},
        'relay.env': {'DATABASE_URL': f'postgres://zoen:{password}@postgres:5432/zoen_relay', 'AWS_ACCESS_KEY_ID': access, 'AWS_SECRET_ACCESS_KEY': secret, 'ZOEN_BACKUP_VAULT_KEY': secrets.token_hex(32), 'ZOEN_METRICS_KEY': secrets.token_hex(32), 'ZOEN_ADMIN_TOKEN': secrets.token_hex(32), 'ZOEN_LOG_PSEUDONYM_KEY': secrets.token_hex(32)},
    }
    for file, values in envs.items():
        (path / file).write_text(''.join(k + '=' + v + '\n' for k, v in values.items()))
        (path / file).chmod(0o600)
    config(path, owner)
    return path, owner


def image_keys(owner):
    return ['relay', 'fdb', 'postgres', 'storage', 'archive'] + (['bus'] if owner['nodes'] == 2 else [])


def volume_names(owner):
    return ['postgres', 'fdb', 'objects'] + (['bus'] if owner['nodes'] == 2 else [])


def installed_images(images, owner):
    require(set(images) == set(image_keys(owner)), 'Pin all exact service and backup-tool images first')
    for name, identifier in images.items():
        require(identifier.startswith('sha256:') and len(identifier) == 71, 'Images must be immutable local image IDs')
        value = inspect('image', identifier)
        require(value is not None and value['Id'] == identifier, f'Required exact {name} image absent; restore never substitutes or pulls')


def pin(path, owner, relay=None, fdb=None):
    require(not command(compose(path, owner) + ['ps', '-a', '-q']).strip(), 'Pin before creating this instance; existing containers are never silently upgraded')
    refs = {**IMAGE_REFS, **owner.get('image_refs', {})}
    refs['relay'] = relay or refs['relay']
    refs['fdb'] = fdb or refs['fdb']
    images = {}
    for name in image_keys(owner):
        value = inspect('image', refs[name])
        require(value is not None, f'Pull/build declared {name} image explicitly before pin: {refs[name]}')
        images[name] = value['Id']
    installed_images(images, owner)
    owner['image_refs'], owner['images'] = refs, images
    write_json(path / 'owner.json', owner)
    config(path, owner)
    print('Pinned exact local image IDs; automatic image pulls are disabled')


def config(path, owner):
    images = owner.get('images', {})
    refs = {**IMAGE_REFS, **owner.get('image_refs', {})}
    values = {'PROJECT_NAME': owner['project'], 'OWNER_NONCE': owner['nonce'], 'STATE_DIR': str(path), 'BIND_PORT': owner['port'], 'SECOND_PORT': owner['port'] + 1}
    values.update({key.upper() + '_IMAGE': images.get(key, refs[key]) for key in IMAGE_REFS})
    (path / 'compose.env').write_text(''.join(k + '=' + str(v) + '\n' for k, v in values.items()))
    (path / 'compose.env').chmod(0o600)


def ready(owner, timeout=180):
    end = time.monotonic() + timeout
    ports = [owner['port']] + ([owner['port'] + 1] if owner['nodes'] == 2 else [])
    while True:
        try:
            for port in ports:
                with urllib.request.urlopen(f'http://127.0.0.1:{port}/readyz', timeout=3) as response:
                    require(response.status == 200 and response.read() == b'ready', 'Native readiness rejected')
            return
        except Exception:
            if time.monotonic() >= end:
                raise TimeoutError('Real native /readyz did not become ready')
            time.sleep(1)


def up(path, owner):
    owned(path, owner)
    installed_images(owner.get('images', {}), owner)
    current = command(compose(path, owner) + ['ps', '-q']).strip()
    if not current:
        for port in [owner['port']] + ([owner['port'] + 1] if owner['nodes'] == 2 else []):
            with socket.socket() as listener:
                listener.bind(('127.0.0.1', port))
    command(compose(path, owner) + ['up', '-d', '--wait', '--wait-timeout', '180'], capture=False, timeout=240)
    ready(owner)
    print(f'Native relay ready: http://127.0.0.1:{owner["port"]}')


def stop(path, owner):
    owned(path, owner)
    command(compose(path, owner) + ['stop', '--timeout', '45'], capture=False, timeout=180)
    require(not command(compose(path, owner) + ['ps', '--status', 'running', '-q']).strip(), 'Cold snapshot requires every owned service stopped')


def archive_command(owner, source, target, args, name, owned_output=None):
    base = DOCKER + ['run', '--name', name, '--rm', '--pull', 'never', '--network', 'none', '--label', 'purpose=archive', '--label', 'app=zoen.local', '--label', 'owner=' + owner['nonce'], '-v', source, '-v', target]
    if owned_output:
        # Root must read database-owned files, then return the archive to the host
        # operator. chmod alone cannot fix root-owned bind-mount files on Linux.
        return base + ['-e', 'LOCAL_UID=' + str(os.getuid()), '-e', 'LOCAL_GID=' + str(os.getgid()), '-e', 'OUTPUT=' + owned_output, '--entrypoint', '/bin/sh', owner['images']['archive'], '-ec', 'umask 077; tar "$@"; chown "$LOCAL_UID:$LOCAL_GID" "$OUTPUT"; chmod 0600 "$OUTPUT"', 'archive'] + args
    return base + ['--entrypoint', 'tar', owner['images']['archive']] + args


def remove_owned_helper(value, owner, purpose):
    labels = value['Config'].get('Labels', {})
    require(labels.get('app') == 'zoen.local' and labels.get('owner') == owner['nonce'] and labels.get('purpose') == purpose, 'Never clean up a foreign ' + purpose + ' helper')
    try:
        command(DOCKER + ['rm', '-f', value['Id']])
    except RuntimeError:
        # --rm may have completed between inspect and rm. Never remove a new
        # container that happens to reuse the helper's name.
        if inspect('container', value['Id']) is not None:
            raise


def archive(owner, source, target, args, owned_output=None):
    name = owner['project'] + '-archive-' + secrets.token_hex(6)
    require(inspect('container', name) is None, 'Archive helper name already exists')
    try:
        command(archive_command(owner, source, target, args, name, owned_output), timeout=180)
    finally:
        value = inspect('container', name)
        if value:
            remove_owned_helper(value, owner, 'archive')


def backup(path, owner):
    owned(path, owner)
    installed_images(owner.get('images', {}), owner)
    target = path / 'backups' / (time.strftime('%Y%m%dT%H%M%S') + '-' + secrets.token_hex(3))
    target.mkdir(parents=True, mode=0o700)
    stop(path, owner)
    manifest = {'schema': 1, 'purpose': PURPOSE, 'source_instance': owner, 'cold': True, 'volumes': [], 'private': [], 'images': dict(owner['images'])}
    try:
        for service in ['relay', 'postgres', 'fdb', 'storage'] + (['relay2', 'bus'] if owner['nodes'] == 2 else []):
            ids = command(compose(path, owner) + ['ps', '-a', '-q', service]).strip().splitlines()
            require(len(ids) == 1, f'Expected one owned {service} container for cold backup')
            actual = json.loads(command(DOCKER + ['inspect', ids[0]]))[0]['Image']
            require(actual == owner['images']['relay' if service == 'relay2' else service], 'Container image differs from pinned image')
        for suffix in volume_names(owner):
            name = owner['project'] + '_' + suffix
            require(inspect('volume', name) is not None, 'Missing owned volume')
            archive(owner, name + ':/source:ro', str(target) + ':/backup', ['-cf', '/backup/' + suffix + '.tar', '-C', '/source', '.'], owned_output='/backup/' + suffix + '.tar')
            saved = target / (suffix + '.tar')
            saved.chmod(0o600)
            manifest['volumes'].append({'name': suffix, 'bytes': saved.stat().st_size, 'sha256': sha256(saved)})
        private = target / 'private'
        private.mkdir(mode=0o700)
        for file in PRIVATE_FILES:
            shutil.copyfile(path / file, private / file)
            (private / file).chmod(0o600)
            manifest['private'].append({'name': file, 'bytes': (private / file).stat().st_size, 'sha256': sha256(private / file)})
        write_json(target / 'manifest.json', manifest)
    finally:
        up(path, owner)
    print('Cold backup created: ' + str(target))


def validate_backup(source, manifest):
    require(manifest['schema'] == 1 and manifest['purpose'] == PURPOSE and manifest['cold'] is True, 'Invalid cold backup')
    old = manifest['source_instance']
    old_path, actual = instance(old['name'])
    for key in ['schema', 'purpose', 'name', 'root', 'project', 'nonce']:
        require(old[key] == actual[key], 'Backup owner identity differs')
    require(source.is_relative_to(old_path / 'backups'), 'Only this app-owned backup root')
    require(set(row['name'] for row in manifest['volumes']) == set(volume_names(old)) and len(manifest['volumes']) == len(volume_names(old)), 'Incomplete or duplicated volume inventory')
    require(set(row['name'] for row in manifest['private']) == set(PRIVATE_FILES) and len(manifest['private']) == len(PRIVATE_FILES), 'Incomplete or duplicated secret inventory')
    for rows, parent, extension in [(manifest['volumes'], source, '.tar'), (manifest['private'], source / 'private', '')]:
        for row in rows:
            p = parent / (row['name'] + extension)
            require(p.is_file() and not p.is_symlink() and p.stat().st_mode & 0o777 == 0o600, 'Backup files must be regular private files')
            require(p.stat().st_size == row['bytes'] and sha256(p) == row['sha256'], 'Backup integrity mismatch; no resources created')
    installed_images(manifest['images'], old)
    return old


def restore(source, name, port):
    source = source.resolve()
    manifest = json.loads((source / 'manifest.json').read_text())
    old = validate_backup(source, manifest)  # All images, vault keys and data checked before init/volume creation.
    path, owner = init(name, port, old['nodes'], old['image_refs']['relay'], old['image_refs']['fdb'])
    owner['images'], owner['image_refs'] = manifest['images'], old['image_refs']
    write_json(path / 'owner.json', owner)
    config(path, owner)
    for file in PRIVATE_FILES:
        shutil.copyfile(source / 'private' / file, path / file)
        (path / file).chmod(0o600)
    for row in manifest['volumes']:
        volume = owner['project'] + '_' + row['name']
        require(inspect('volume', volume) is None, 'Restore never overwrites existing volumes')
        command(DOCKER + ['volume', 'create', '--label', 'app=zoen.local', '--label', 'owner=' + owner['nonce'], volume])
        archive(owner, volume + ':/target', str(source) + ':/backup:ro', ['-xf', '/backup/' + row['name'] + '.tar', '-C', '/target'])
    write_json(path / 'restored-from.json', {'backup': str(source), 'manifest_sha256': sha256(source / 'manifest.json'), 'original_volumes_untouched': True})
    up(path, owner)


def cluster(path, owner):
    require(owner['nodes'] == 1, 'Instance already has two nodes')
    require(owner['port'] + 1 not in [8790, 5547, 8089], 'Protected second port')
    installed_images(owner['images'], owner)
    value = inspect('image', IMAGE_REFS['bus'])
    require(value is not None, 'Pull the declared NATS image explicitly first')
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', owner['port'] + 1))
    command(compose(path, owner) + ['stop', '--timeout', '45', 'relay'], capture=False, timeout=100)
    owner['nodes'] = 2
    owner['images']['bus'] = value['Id']
    owner['image_refs']['bus'] = IMAGE_REFS['bus']
    write_json(path / 'owner.json', owner)
    config(path, owner)
    up(path, owner)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['init', 'pin', 'up', 'status', 'drain', 'restart', 'down', 'backup', 'restore', 'cluster'])
    parser.add_argument('--instance', default='default')
    parser.add_argument('--port', type=int, default=8880)
    parser.add_argument('--nodes', type=int, choices=[1, 2], default=1)
    parser.add_argument('--relay-image')
    parser.add_argument('--fdb-image')
    parser.add_argument('--backup', type=pathlib.Path)
    args = parser.parse_args()
    if args.action == 'init':
        init(args.instance, args.port, args.nodes, args.relay_image or IMAGE_REFS['relay'], args.fdb_image or IMAGE_REFS['fdb'])
        return
    if args.action == 'restore':
        require(args.backup is not None, 'Specify --backup')
        restore(args.backup, args.instance, args.port)
        return
    path, owner = instance(args.instance)
    owned(path, owner)
    if args.action == 'pin':
        pin(path, owner, args.relay_image, args.fdb_image)
    elif args.action == 'up':
        up(path, owner)
    elif args.action == 'status':
        command(compose(path, owner) + ['ps'], capture=False)
        ready(owner, 3)
        print('Readiness passed')
    elif args.action == 'drain':
        command(compose(path, owner) + ['stop', '--timeout', '45', 'relay'] + (['relay2'] if owner['nodes'] == 2 else []), capture=False, timeout=100)
    elif args.action == 'down':
        command(compose(path, owner) + ['down', '--timeout', '45'], capture=False, timeout=180)  # Volumes retained.
    elif args.action == 'restart':
        stop(path, owner)
        up(path, owner)
    elif args.action == 'backup':
        backup(path, owner)
    elif args.action == 'cluster':
        cluster(path, owner)


if __name__ == '__main__':
    main()
