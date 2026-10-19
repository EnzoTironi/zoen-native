#!/usr/bin/env python3
"""Manage only this instance's native server, private dependencies and persistent volumes."""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import pathlib
import secrets
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request

REPO = pathlib.Path(__file__).resolve().parents[2]
ROOT = REPO / '.dev' / 'local'
DOCKER = ['docker', '--context', 'orbstack'] if sys.platform == 'darwin' else ['docker']
PURPOSE = 'native-local-environment'
PRIVATE_FILES = ('postgres.env', 'storage.env', 'relay.env')
IMAGE_REFS = {
    'relay': 'zoen-local-relay:current',
    'fdb': 'zoen-local-fdb:current',
    'postgres': 'postgres:17-bookworm',
    'storage': 'chrislusf/seaweedfs:4.48@sha256:4e61d15fd35994cb1e43e1e553dff106794841fd9a99ade2fc8c8bfce4d7872d',
    'archive': 'debian:bookworm-slim',
    'bus': 'nats:2.15.0-alpine',
    'frontdoor': 'mirror.gcr.io/library/haproxy@sha256:251da69821897ec8c4819f12b1babbc4b71f562943addc9b9eb14bb6ece40dc6',
}


def frontdoor_info(owner):
    info = owner.get('frontdoor')
    if info is None:
        return None
    require(isinstance(info, dict) and set(info) == {'port', 'config_sha256'}, 'Invalid frontdoor configuration')
    require(owner['nodes'] == 2, 'Frontdoor requires two native relays')
    validate_frontdoor_port(owner, info['port'])
    require(isinstance(info['config_sha256'], str) and len(info['config_sha256']) == 64 and all(c in '0123456789abcdef' for c in info['config_sha256']), 'Invalid pinned frontdoor hash')
    return info


def validate_frontdoor_port(owner, port):
    require(type(port) is int and 1024 <= port <= 65535 and port not in [8790, 5547, 8089, owner['port'], owner['port'] + 1], 'Choose an unprotected distinct frontdoor port')


def frontdoor_config(path, owner):
    info = frontdoor_info(owner)
    if info is None:
        return None
    source = path / 'haproxy.cfg'
    require(source.is_file() and not source.is_symlink() and source.stat().st_uid == os.getuid() and source.stat().st_mode & 0o777 == 0o444, 'Pinned frontdoor config must be owned, regular and read-only')
    require(sha256(source) == info['config_sha256'], 'Pinned frontdoor config integrity mismatch')
    return source


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


def instance(name, check_runtime=True):
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
    if check_runtime:
        frontdoor_config(path, owner)
    return path, owner


def compose(path, owner, env_file=None):
    result = DOCKER + ['compose', '--project-name', owner['project'], '--env-file', str(env_file or path / 'compose.env'), '-f', str(REPO / 'infra/local/compose.yaml')]
    if owner['nodes'] == 2:
        result += ['-f', str(REPO / 'infra/local/compose.cluster.yaml')]
    if frontdoor_info(owner):
        result += ['-f', str(REPO / 'infra/local/compose.frontdoor.yaml')]
    return result


def owned(path, owner, check_config=True):
    frontdoor_config(path, owner)
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
    if check_config:
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
    return ['relay', 'fdb', 'postgres', 'storage', 'archive'] + (['bus'] if owner['nodes'] == 2 else []) + (['frontdoor'] if frontdoor_info(owner) else [])


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
    frontdoor_config(path, owner)
    refs = {**IMAGE_REFS, **owner.get('image_refs', {})}
    refs['relay'] = relay or refs['relay']
    refs['fdb'] = fdb or refs['fdb']
    if frontdoor_info(owner):
        refs['frontdoor'] = IMAGE_REFS['frontdoor']
    images = {}
    for name in image_keys(owner):
        value = inspect('image', refs[name])
        require(value is not None, f'Pull/build declared {name} image explicitly before pin: {refs[name]}')
        if name == 'frontdoor':
            require(refs[name] in value.get('RepoDigests', []), 'Frontdoor image must match its declared immutable index')
        images[name] = value['Id']
    installed_images(images, owner)
    if frontdoor_info(owner):
        data = (REPO / 'infra/local/haproxy.cfg').read_bytes()
        with tempfile.TemporaryDirectory(prefix='frontdoor-check-', dir=path) as temporary:
            candidate = pathlib.Path(temporary) / 'haproxy.cfg'
            candidate.write_bytes(data)
            candidate.chmod(0o444)
            check_frontdoor_parser(owner, images['frontdoor'], candidate)
        publish_frontdoor_config(path, data, replace_pinned=True)
        owner['frontdoor']['config_sha256'] = hashlib.sha256(data).hexdigest()
    owner['image_refs'], owner['images'] = refs, images
    write_json(path / 'owner.json', owner)
    config(path, owner)
    print('Pinned exact local image IDs; automatic image pulls are disabled')


def config(path, owner, output=None):
    images = owner.get('images', {})
    refs = {**IMAGE_REFS, **owner.get('image_refs', {})}
    values = {'PROJECT_NAME': owner['project'], 'OWNER_NONCE': owner['nonce'], 'STATE_DIR': str(path), 'BIND_PORT': owner['port'], 'SECOND_PORT': owner['port'] + 1}
    if frontdoor_info(owner):
        values['FRONTDOOR_PORT'] = owner['frontdoor']['port']
        values['FRONTDOOR_CONFIG_SHA256'] = owner['frontdoor']['config_sha256']
    values.update({key.upper() + '_IMAGE': images.get(key, refs[key]) for key in IMAGE_REFS})
    output = output or path / 'compose.env'
    output.write_text(''.join(k + '=' + str(v) + '\n' for k, v in values.items()))
    output.chmod(0o600)


def ready(owner, timeout=180, include_frontdoor=True):
    end = time.monotonic() + timeout
    ports = [owner['port']] + ([owner['port'] + 1] if owner['nodes'] == 2 else [])
    if include_frontdoor and frontdoor_info(owner):
        ports.append(owner['frontdoor']['port'])
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
        ports = [owner['port']] + ([owner['port'] + 1] if owner['nodes'] == 2 else []) + ([owner['frontdoor']['port']] if frontdoor_info(owner) else [])
        for port in ports:
            with socket.socket() as listener:
                listener.bind(('127.0.0.1', port))
    command(compose(path, owner) + ['up', '-d', '--wait', '--wait-timeout', '180'], capture=False, timeout=240)
    ready(owner)
    print(f'Native relay ready: http://127.0.0.1:{owner["port"]}')


def stop(path, owner):
    owned(path, owner)
    stop_frontdoor(path, owner)
    command(compose(path, owner) + ['stop', '--timeout', '45'], capture=False, timeout=180)
    require(not command(compose(path, owner) + ['ps', '--status', 'running', '-q']).strip(), 'Cold snapshot requires every owned service stopped')


def publish_frontdoor_config(path, data, replace_pinned=False):
    target = path / 'haproxy.cfg'
    if target.exists() or target.is_symlink():
        require(target.is_file() and not target.is_symlink() and target.stat().st_uid == os.getuid() and target.stat().st_mode & 0o777 == 0o444, 'Refuse foreign frontdoor config')
        require(replace_pinned or target.read_bytes() == data, 'Refuse to replace unpinned frontdoor config')
    descriptor, temporary = tempfile.mkstemp(prefix='frontdoor-config-', dir=path)
    try:
        with os.fdopen(descriptor, 'wb') as output:
            output.write(data)
        os.chmod(temporary, 0o444)  # The unprivileged official container must read it.
        os.replace(temporary, target)
    finally:
        pathlib.Path(temporary).unlink(missing_ok=True)


def check_frontdoor_parser(owner, image, source):
    name = owner['project'] + '-frontdoor-check-' + secrets.token_hex(6)
    require(inspect('container', name) is None, 'Frontdoor parser helper already exists')
    try:
        command(DOCKER + ['run', '--name', name, '--rm', '--pull', 'never', '--network', 'none', '--read-only', '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges:true', '--memory', '128m', '--cpus', '0.5', '--label', 'app=zoen.local', '--label', 'owner=' + owner['nonce'], '--label', 'purpose=frontdoor-check', '-v', str(source) + ':/usr/local/etc/haproxy/haproxy.cfg:ro', image, 'haproxy', '-c', '-f', '/usr/local/etc/haproxy/haproxy.cfg'], timeout=30)
    finally:
        value = inspect('container', name)
        if value:
            remove_owned_helper(value, owner, 'frontdoor-check')


def native_snapshot(path, owner):
    rows = []
    for identifier in command(compose(path, owner) + ['ps', '-a', '-q']).splitlines():
        value = inspect('container', identifier)
        require(value is not None, 'Native container disappeared')
        labels = value['Config']['Labels']
        require(labels.get('app') == 'zoen.local' and labels.get('owner') == owner['nonce'], 'Foreign native container')
        service = labels.get('com.docker.compose.service')
        if service == 'frontdoor':
            validate_frontdoor_container(value, owner)
            continue
        require(service in ['relay', 'relay2', 'bus', 'postgres', 'fdb', 'storage'], 'Unexpected native service')
        require(value['Image'] == owner['images']['relay' if service == 'relay2' else service], 'Native service image differs from pin')
        require(value['State']['Running'] and value['State'].get('Health', {}).get('Status') == 'healthy', 'Both native relays and dependencies must be healthy')
        rows.append({'service': service, 'id': value['Id'], 'pid': value['State']['Pid'], 'image': value['Image'], 'config_sha256': hashlib.sha256(json.dumps(value['Config'], sort_keys=True).encode()).hexdigest()})
    require(len(rows) == 6 and {row['service'] for row in rows} == {'relay', 'relay2', 'bus', 'postgres', 'fdb', 'storage'}, 'Expected exactly six healthy native services')
    return sorted(rows, key=lambda row: row['service'])


def validate_frontdoor_container(value, owner):
    info = frontdoor_info(owner)
    require(info is not None and value is not None, 'Untracked frontdoor container')
    labels = value['Config']['Labels']
    require(labels.get('app') == 'zoen.local' and labels.get('owner') == owner['nonce'] and labels.get('purpose') == 'native-local-frontdoor' and labels.get('com.docker.compose.service') == 'frontdoor' and labels.get('config_sha256') == info['config_sha256'], 'Foreign or differently configured frontdoor')
    require(value['Image'] == owner['images']['frontdoor'], 'Frontdoor image differs from pin')
    require(value['HostConfig']['PortBindings'] == {'8080/tcp': [{'HostIp': '127.0.0.1', 'HostPort': str(info['port'])}]}, 'Frontdoor binding differs from its canonical loopback port')


def stop_frontdoor(path, owner, remove=False):
    if frontdoor_info(owner) is None:
        return
    for identifier in command(compose(path, owner) + ['ps', '-a', '-q', 'frontdoor']).splitlines():
        value = inspect('container', identifier)
        validate_frontdoor_container(value, owner)
        if value['State']['Running']:
            command(DOCKER + ['stop', '--time', '10', value['Id']], capture=False, timeout=20)
        if remove:
            command(DOCKER + ['rm', value['Id']], capture=False, timeout=15)


def frontdoor(path, owner, port=None, disable=False):
    # Recreate generated Compose input after an interrupted enabled start. Never
    # erase the durable enabled descriptor after an ambiguous Docker up result.
    owned(path, owner, check_config=False)
    installed_images(owner.get('images', {}), owner)
    info = frontdoor_info(owner)
    config(path, owner)
    if disable:
        require(info is not None, 'Frontdoor is not enabled')
        stop_frontdoor(path, owner, remove=True)
        owner.pop('frontdoor')
        owner['images'].pop('frontdoor')
        owner['image_refs'].pop('frontdoor', None)
        write_json(path / 'owner.json', owner)
        config(path, owner)
        (path / 'haproxy.cfg').unlink()
        return
    require(owner['nodes'] == 2, 'Frontdoor requires two native relays')
    selected = info['port'] if info else (owner['port'] + 2 if port is None else port)
    validate_frontdoor_port(owner, selected)
    require(info is None or port is None or selected == port, 'Disable frontdoor explicitly before changing its canonical port')
    before = native_snapshot(path, owner)
    ready(owner, 3, include_frontdoor=False)
    prospective = copy.deepcopy(owner)
    with tempfile.TemporaryDirectory(prefix='frontdoor-prepare-', dir=path) as temporary:
        temporary = pathlib.Path(temporary)
        if info is None:
            value = inspect('image', IMAGE_REFS['frontdoor'])
            require(value is not None and IMAGE_REFS['frontdoor'] in value.get('RepoDigests', []), 'Pull the exact declared standard frontdoor image first')
            prospective['images']['frontdoor'] = value['Id']
            prospective['image_refs']['frontdoor'] = IMAGE_REFS['frontdoor']
            data = (REPO / 'infra/local/haproxy.cfg').read_bytes()
            prospective['frontdoor'] = {'port': selected, 'config_sha256': hashlib.sha256(data).hexdigest()}
            candidate = temporary / 'haproxy.cfg'
            candidate.write_bytes(data)
            candidate.chmod(0o444)
        else:
            candidate = frontdoor_config(path, owner)
            data = candidate.read_bytes()
        installed_images(prospective['images'], prospective)
        # A running owned retry already holds the port; otherwise it must be free.
        current = command(compose(path, owner) + ['ps', '-q', 'frontdoor']).strip() if info else ''
        if not current:
            with socket.socket() as listener:
                listener.bind(('127.0.0.1', selected))
        check_frontdoor_parser(prospective, prospective['images']['frontdoor'], candidate)
        config(path, prospective, output=temporary / 'compose.env')
        command(compose(path, prospective, env_file=temporary / 'compose.env') + ['config', '--quiet'])
        if info is None:
            publish_frontdoor_config(path, data)
            owner.update(prospective)
            write_json(path / 'owner.json', owner)
        config(path, owner)
    # The only runtime target is this gateway. Existing native processes/configs
    # remain unchanged; errors retain the pin/config and permit this exact retry.
    command(compose(path, owner) + ['up', '-d', '--no-deps', '--wait', '--wait-timeout', '60', 'frontdoor'], capture=False, timeout=90)
    ready(owner, 20)
    ids = command(compose(path, owner) + ['ps', '-a', '-q', 'frontdoor']).splitlines()
    require(len(ids) == 1, 'Expected exactly one owned frontdoor')
    actual = inspect('container', ids[0])
    validate_frontdoor_container(actual, owner)
    require(actual['State']['Running'] and actual['State'].get('Health', {}).get('Status') == 'healthy', 'Frontdoor did not become healthy')
    after = native_snapshot(path, owner)
    require(before == after, 'Native services changed during frontdoor-only activation')
    write_json(path / 'frontdoor-runtime.json', {'port': selected, 'image': owner['images']['frontdoor'], 'config_sha256': owner['frontdoor']['config_sha256'], 'native_before': before, 'native_after': after, 'cross_process_journey_accepted': False})
    print(f'Native frontdoor ready: http://127.0.0.1:{selected}')


def down(path, owner):
    owned(path, owner)
    stop_frontdoor(path, owner, remove=True)
    command(compose(path, owner) + ['down', '--timeout', '45'], capture=False, timeout=180)  # Volumes retained.


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
    manifest = {'schema': 1, 'purpose': PURPOSE, 'source_instance': copy.deepcopy(owner), 'cold': True, 'volumes': [], 'private': [], 'runtime': [], 'images': dict(owner['images'])}
    try:
        for service in ['relay', 'postgres', 'fdb', 'storage'] + (['relay2', 'bus'] if owner['nodes'] == 2 else []) + (['frontdoor'] if frontdoor_info(owner) else []):
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
        if frontdoor_info(owner):
            runtime = target / 'runtime'
            runtime.mkdir(mode=0o700)
            source = frontdoor_config(path, owner)
            shutil.copyfile(source, runtime / 'haproxy.cfg')
            (runtime / 'haproxy.cfg').chmod(0o600)
            manifest['runtime'].append({'name': 'haproxy.cfg', 'bytes': source.stat().st_size, 'sha256': sha256(source)})
        write_json(target / 'manifest.json', manifest)
    finally:
        up(path, owner)
    print('Cold backup created: ' + str(target))


def validate_backup(source, manifest):
    require(manifest['schema'] == 1 and manifest['purpose'] == PURPOSE and manifest['cold'] is True, 'Invalid cold backup')
    old = manifest['source_instance']
    # An intact backup must remain recoverable when the current runtime config
    # is damaged or missing. Owner identity/private-file ownership stay strict;
    # the archived config is independently hash-checked below before resources.
    old_path, actual = instance(old['name'], check_runtime=False)
    for key in ['schema', 'purpose', 'name', 'root', 'project', 'nonce']:
        require(old[key] == actual[key], 'Backup owner identity differs')
    require(source.is_relative_to(old_path / 'backups'), 'Only this app-owned backup root')
    require(set(row['name'] for row in manifest['volumes']) == set(volume_names(old)) and len(manifest['volumes']) == len(volume_names(old)), 'Incomplete or duplicated volume inventory')
    require(set(row['name'] for row in manifest['private']) == set(PRIVATE_FILES) and len(manifest['private']) == len(PRIVATE_FILES), 'Incomplete or duplicated secret inventory')
    runtime = manifest.get('runtime', [])
    require(len(runtime) == (1 if frontdoor_info(old) else 0) and {row['name'] for row in runtime} == ({'haproxy.cfg'} if frontdoor_info(old) else set()), 'Incomplete or duplicated frontdoor config inventory')
    if runtime:
        require(runtime[0]['sha256'] == old['frontdoor']['config_sha256'], 'Backup frontdoor hash differs from its pin')
    for rows, parent, extension in [(manifest['volumes'], source, '.tar'), (manifest['private'], source / 'private', ''), (runtime, source / 'runtime', '')]:
        for row in rows:
            p = parent / (row['name'] + extension)
            require(p.is_file() and not p.is_symlink() and p.stat().st_mode & 0o777 == 0o600, 'Backup files must be regular private files')
            require(p.stat().st_size == row['bytes'] and sha256(p) == row['sha256'], 'Backup integrity mismatch; no resources created')
    installed_images(manifest['images'], old)
    return old


def restore(source, name, port, frontdoor_port=None):
    source = source.resolve()
    manifest = json.loads((source / 'manifest.json').read_text())
    old = validate_backup(source, manifest)  # All images, vault keys and data checked before init/volume creation.
    if frontdoor_info(old):
        selected = port + 2 if frontdoor_port is None else frontdoor_port
        validate_frontdoor_port({**old, 'port': port}, selected)
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', selected))
    else:
        require(frontdoor_port is None, 'Backup has no enabled frontdoor')
    path, owner = init(name, port, old['nodes'], old['image_refs']['relay'], old['image_refs']['fdb'])
    owner['images'], owner['image_refs'] = manifest['images'], old['image_refs']
    if frontdoor_info(old):
        owner['frontdoor'] = {**old['frontdoor'], 'port': selected}
        publish_frontdoor_config(path, (source / 'runtime/haproxy.cfg').read_bytes())
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
    parser.add_argument('action', choices=['init', 'pin', 'up', 'status', 'drain', 'restart', 'down', 'backup', 'restore', 'cluster', 'frontdoor'])
    parser.add_argument('--instance', default='default')
    parser.add_argument('--port', type=int, default=8880)
    parser.add_argument('--nodes', type=int, choices=[1, 2], default=1)
    parser.add_argument('--relay-image')
    parser.add_argument('--fdb-image')
    parser.add_argument('--backup', type=pathlib.Path)
    parser.add_argument('--frontdoor-port', type=int)
    parser.add_argument('--disable', action='store_true', help='Disable only the optional frontdoor')
    args = parser.parse_args()
    require(not args.disable or args.action == 'frontdoor', '--disable applies only to frontdoor')
    require(args.frontdoor_port is None or args.action in ['frontdoor', 'restore'], 'Frontdoor port applies only to frontdoor or restore')
    if args.action == 'init':
        init(args.instance, args.port, args.nodes, args.relay_image or IMAGE_REFS['relay'], args.fdb_image or IMAGE_REFS['fdb'])
        return
    if args.action == 'restore':
        require(args.backup is not None, 'Specify --backup')
        require(not args.disable, '--disable applies only to frontdoor')
        restore(args.backup, args.instance, args.port, args.frontdoor_port)
        return
    path, owner = instance(args.instance)
    if args.action == 'frontdoor':
        require(not args.disable or args.frontdoor_port is None, 'Disable does not change the frontdoor port')
        frontdoor(path, owner, args.frontdoor_port, args.disable)
        return
    require(not args.disable and args.frontdoor_port is None, 'Frontdoor options apply only to frontdoor or restore')
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
        stop_frontdoor(path, owner)
        command(compose(path, owner) + ['stop', '--timeout', '45', 'relay'] + (['relay2'] if owner['nodes'] == 2 else []), capture=False, timeout=100)
    elif args.action == 'down':
        down(path, owner)
    elif args.action == 'restart':
        stop(path, owner)
        up(path, owner)
    elif args.action == 'backup':
        backup(path, owner)
    elif args.action == 'cluster':
        cluster(path, owner)


if __name__ == '__main__':
    main()
