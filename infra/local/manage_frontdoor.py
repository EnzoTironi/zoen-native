"""Owned frontdoor and relay shutdown operations."""
from __future__ import annotations

import copy
import hashlib
import json
import os
import pathlib
import secrets
import tempfile

from manage_runtime import (
    DOCKER, IMAGE_REFS, REPO, command, compose, config, frontdoor_config, frontdoor_info,
    inspect, installed_images, owned, ready, remove_owned_helper, require, require_available_port,
    sha256, validate_frontdoor_port, write_json,
)


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
            require_available_port(selected)
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
