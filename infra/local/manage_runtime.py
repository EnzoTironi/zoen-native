"""Runtime and ownership checks for the local relay."""
from __future__ import annotations

import hashlib
import json
import os
import pathlib
import secrets
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


def require_available_port(port):
    with socket.socket() as listener:
        # Permit a stopped server's TIME_WAIT sockets, but never share a live
        # listener. listen() also rejects an existing SO_REUSEADDR listener.
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(('127.0.0.1', port))
        listener.listen(1)


def up(path, owner):
    owned(path, owner)
    installed_images(owner.get('images', {}), owner)
    current = command(compose(path, owner) + ['ps', '-q']).strip()
    if not current:
        ports = [owner['port']] + ([owner['port'] + 1] if owner['nodes'] == 2 else []) + ([owner['frontdoor']['port']] if frontdoor_info(owner) else [])
        for port in ports:
            require_available_port(port)
    command(compose(path, owner) + ['up', '-d', '--wait', '--wait-timeout', '180'], capture=False, timeout=240)
    ready(owner)
    print(f'Native relay ready: http://127.0.0.1:{owner["port"]}')


def cluster(path, owner):
    require(owner['nodes'] == 1, 'Instance already has two nodes')
    require(owner['port'] + 1 not in [8790, 5547, 8089], 'Protected second port')
    installed_images(owner['images'], owner)
    value = inspect('image', IMAGE_REFS['bus'])
    require(value is not None, 'Pull the declared NATS image explicitly first')
    require_available_port(owner['port'] + 1)
    command(compose(path, owner) + ['stop', '--timeout', '45', 'relay'], capture=False, timeout=100)
    owner['nodes'] = 2
    owner['images']['bus'] = value['Id']
    owner['image_refs']['bus'] = IMAGE_REFS['bus']
    write_json(path / 'owner.json', owner)
    config(path, owner)
    up(path, owner)


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
