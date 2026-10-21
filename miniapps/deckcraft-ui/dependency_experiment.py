"""Opt-in, hash-pinned dependency experiment in a fresh managed source tree only."""
import hashlib
import json
from pathlib import PurePosixPath
import tarfile


def digest(data):
    return hashlib.sha256(data).hexdigest()


def tree_manifest(root):
    rows = []
    for path in sorted(root.rglob('*')):
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ValueError('Unexpected dependency member')
        if path.is_file():
            data = path.read_bytes()
            rows.append({'path': str(path.relative_to(root)), 'bytes': len(data), 'sha256': digest(data)})
    return rows


def tree_digest(rows):
    return digest(json.dumps(rows, sort_keys=True, separators=(',', ':')).encode())


def checked(path, expected):
    data = path.read_bytes()
    if digest(data) != expected:
        raise ValueError('Experiment input hash mismatch: ' + str(path))
    return data


def load_experiment(root, name, upstream):
    if name != 'eframe-shift':
        raise ValueError('Unknown dependency experiment')
    data = json.loads((root / 'experiments' / (name + '.json')).read_text())
    if (data.get('schema') != 1 or data.get('name') != name
            or data.get('upstreamRevision') != upstream['revision']
            or data.get('baseManagedCargoLockSha256') != upstream['managedCargoLockSha256']):
        raise ValueError('Experiment base changed')
    crate = data['crate']
    if (crate['name'] != 'eframe' or crate['version'] != '0.36.2'
            or crate['prefix'] != 'eframe-0.36.2'
            or crate['vendorPath'] != 'vendor/eframe-0.36.2'
            or crate['changedFiles'] != ['src/web/text_agent.rs']):
        raise ValueError('Unexpected experiment dependency')
    return data


def stage_experiment(source, metadata, archive, package_root, apply_patch):
    """Never reads or modifies an extracted shared Cargo registry source."""
    checked(source / 'Cargo.lock', metadata['baseManagedCargoLockSha256'])
    for item in metadata['sourceFiles']:
        checked(source / item['path'], item['before'])
    crate = metadata['crate']
    checked(archive, crate['archiveSha256'])
    vendor = source / crate['vendorPath']
    vendor.mkdir(parents=True, exist_ok=False)
    total = 0
    seen = set()
    with tarfile.open(archive, 'r:gz') as tar:
        members = tar.getmembers()
        if not 1 <= len(members) <= 4096:
            raise ValueError('Invalid dependency archive member count')
        for member in members:
            name = PurePosixPath(member.name)
            if (name.is_absolute() or len(name.parts) < 2 or name.parts[0] != crate['prefix']
                    or '..' in name.parts or '\\' in member.name or not member.isfile()
                    or member.name in seen or not 0 <= member.size <= 8 * 1024 * 1024):
                raise ValueError('Unsafe dependency archive member')
            seen.add(member.name)
            total += member.size
            if total > 32 * 1024 * 1024:
                raise ValueError('Dependency archive too large')
            relative = PurePosixPath(*name.parts[1:])
            destination = vendor.joinpath(*relative.parts)
            destination.parent.mkdir(parents=True, exist_ok=True)
            data = tar.extractfile(member).read()
            if len(data) != member.size:
                raise ValueError('Dependency archive length changed')
            with destination.open('xb') as output:
                output.write(data)
    before = tree_manifest(vendor)
    if tree_digest(before) != crate['beforeTreeSha256']:
        raise ValueError('Dependency source tree mismatch')
    vcs = json.loads((vendor / '.cargo_vcs_info.json').read_text())
    if vcs.get('git', {}).get('sha1') != crate['sourceRevision']:
        raise ValueError('Dependency source revision mismatch')
    patch_path = PurePosixPath(metadata['patch']['path'])
    if patch_path.is_absolute() or '..' in patch_path.parts:
        raise ValueError('Unsafe experiment patch path')
    patch = package_root.joinpath(*patch_path.parts)
    checked(patch, metadata['patch']['sha256'])
    apply_patch(source, patch)
    checked(source / 'Cargo.lock', metadata['managedCargoLockSha256'])
    for item in metadata['sourceFiles']:
        checked(source / item['path'], item['after'])
    after = tree_manifest(vendor)
    before_by_path = {item['path']: item for item in before}
    after_by_path = {item['path']: item for item in after}
    changed = sorted(name for name in set(before_by_path) | set(after_by_path)
                     if before_by_path.get(name) != after_by_path.get(name))
    if changed != crate['changedFiles'] or tree_digest(after) != crate['afterTreeSha256']:
        raise ValueError('Unexpected patched dependency tree')
    return {'name': metadata['name'], 'status': metadata['status'],
            'crate': crate, 'patch': metadata['patch'],
            'managedCargoLockSha256': metadata['managedCargoLockSha256'],
            'verifiedBeforeTree': before, 'verifiedAfterTree': after}
