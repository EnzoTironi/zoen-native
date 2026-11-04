"""Cold backup and restore of owned local relay data."""
from __future__ import annotations

import copy
import json
import os
import secrets
import shutil
import time

from manage_frontdoor import publish_frontdoor_config, stop
from manage_runtime import (
    DOCKER, PRIVATE_FILES, PURPOSE, command, compose, config, frontdoor_config,
    frontdoor_info, init, inspect, installed_images, instance, owned, remove_owned_helper, require,
    require_available_port, sha256, up, validate_frontdoor_port, volume_names,
    write_json,
)


def archive_command(owner, source, target, args, name, owned_output=None):
    base = DOCKER + ['run', '--name', name, '--rm', '--pull', 'never', '--network', 'none', '--label', 'purpose=archive', '--label', 'app=zoen.local', '--label', 'owner=' + owner['nonce'], '-v', source, '-v', target]
    if owned_output:
        # Root must read database-owned files, then return the archive to the host
        # operator. chmod alone cannot fix root-owned bind-mount files on Linux.
        return base + ['-e', 'LOCAL_UID=' + str(os.getuid()), '-e', 'LOCAL_GID=' + str(os.getgid()), '-e', 'OUTPUT=' + owned_output, '--entrypoint', '/bin/sh', owner['images']['archive'], '-ec', 'umask 077; tar "$@"; chown "$LOCAL_UID:$LOCAL_GID" "$OUTPUT"; chmod 0600 "$OUTPUT"', 'archive'] + args
    return base + ['--entrypoint', 'tar', owner['images']['archive']] + args


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
        require_available_port(selected)
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
