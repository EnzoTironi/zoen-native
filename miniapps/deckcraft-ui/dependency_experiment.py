"""Opt-in, hash-pinned dependency experiment in a fresh managed source tree only."""
import hashlib
import json
import re
from pathlib import PurePosixPath
import tarfile


# Each opt-in recipe has an exact dependency and reviewed file boundary. No
# final ios-input metadata or build pins are inferred from these names.
EXPERIMENT_FILES = {
    'eframe-shift': ['src/web/text_agent.rs'],
    'eframe-ios-input': [
        'src/web/app_runner.rs', 'src/web/backend.rs', 'src/web/events.rs',
        'src/web/mod.rs', 'src/web/text_agent.rs', 'src/web/text_input_activation.rs',
    ],
}
EXPERIMENT_NAMES = tuple(EXPERIMENT_FILES)
EXPERIMENT_SOURCE_FILES = {
    'eframe-shift': ['Cargo.toml', 'Cargo.lock'],
    'eframe-ios-input': [
        'Cargo.toml', 'Cargo.lock', 'crates/ui-egui/src/canvas.rs',
        'crates/ui-egui/src/host_phone_landscape_tests.rs',
    ],
}


def is_sha(value, length=64):
    return isinstance(value, str) and re.fullmatch('[0-9a-f]{' + str(length) + '}', value) is not None


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
    if name not in EXPERIMENT_FILES:
        raise ValueError('Unknown dependency experiment')
    data = json.loads((root / 'experiments' / (name + '.json')).read_text())
    if (not isinstance(data, dict) or type(data.get('schema')) is not int
            or data.get('schema') != 1 or data.get('name') != name
            or data.get('upstreamRevision') != upstream['revision']
            or data.get('baseManagedCargoLockSha256') != upstream['managedCargoLockSha256']):
        raise ValueError('Experiment base changed')
    crate = data.get('crate')
    if (not isinstance(crate, dict) or crate.get('name') != 'eframe'
            or crate.get('version') != '0.36.2' or crate.get('prefix') != 'eframe-0.36.2'
            or crate.get('vendorPath') != 'vendor/eframe-0.36.2'
            or crate.get('changedFiles') != EXPERIMENT_FILES[name]):
        raise ValueError('Unexpected experiment dependency')
    sources = data.get('sourceFiles')
    if (not isinstance(sources, list)
            or any(not isinstance(item, dict) or set(item) != {'path', 'before', 'after'}
                   or not is_sha(item['before']) or not is_sha(item['after']) for item in sources)
            or [item['path'] for item in sources] != EXPERIMENT_SOURCE_FILES[name]):
        raise ValueError('Unexpected experiment source files')
    patch = data.get('patch')
    patch_path = patch.get('path') if isinstance(patch, dict) else None
    if (not isinstance(patch_path, str) or '\\' in patch_path
            or PurePosixPath(patch_path).parts[:1] != ('patches',)
            or len(PurePosixPath(patch_path).parts) != 2 or '..' in PurePosixPath(patch_path).parts
            or PurePosixPath(patch_path).as_posix() != patch_path or not patch_path.endswith('.patch')
            or not is_sha(patch.get('sha256'))
            or not isinstance(data.get('status'), str) or not data['status']
            or not is_sha(data.get('managedCargoLockSha256'))
            or any(not is_sha(crate.get(key)) for key in ['archiveSha256', 'beforeTreeSha256', 'afterTreeSha256'])
            or not is_sha(crate.get('sourceRevision'), 40)
            or sources[1]['before'] != data['baseManagedCargoLockSha256']
            or sources[1]['after'] != data['managedCargoLockSha256']):
        raise ValueError('Invalid experiment metadata')
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
