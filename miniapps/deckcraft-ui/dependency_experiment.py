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
    'egui-touch-input': [
        'src/text_selection/text_cursor_state.rs',
        'src/text_selection/touch_input_tests.rs',
    ],
}
EXPERIMENT_NAMES = tuple(EXPERIMENT_FILES)
IOS_INPUT_PATCHES = (
    'patches/0017a-eframe-vendor-binding.patch',
    'patches/0017b-eframe-text-activation.patch',
    'patches/0017c-eframe-primary-contact.patch',
    'patches/0017d-deckcraft-text-admission.patch',
    'patches/0017e-deckcraft-input-contracts.patch',
)
TOUCH_INPUT_PATCHES = (
    'patches/0018-egui-touch-input.patch',
    'patches/0019-notes-owner-contract.patch',
)
EXPERIMENT_CHAINS = (('eframe-shift',), ('eframe-ios-input',),
                     ('eframe-ios-input', 'egui-touch-input'))
EXPERIMENT_SOURCE_FILES = {
    'eframe-shift': ['Cargo.toml', 'Cargo.lock'],
    'eframe-ios-input': [
        'Cargo.toml', 'Cargo.lock', 'crates/ui-egui/src/canvas.rs',
        'crates/ui-egui/src/host_phone_landscape_tests.rs',
    ],
    'egui-touch-input': [
        'Cargo.toml', 'Cargo.lock', 'crates/ui-egui/src/host_phone_landscape_tests.rs',
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


def experiment_patches(data):
    """Retain single-patch recipes; the input family has one closed ordered chain."""
    if 'patches' not in data:
        return [data.get('patch')]
    patches = data['patches']
    expected = {'eframe-ios-input': IOS_INPUT_PATCHES,
                'egui-touch-input': TOUCH_INPUT_PATCHES}.get(data.get('name'))
    if ('patch' in data or expected is None
            or not isinstance(patches, list)
            or any(not isinstance(item, dict) for item in patches)
            or [item.get('path') for item in patches] != list(expected)):
        raise ValueError('Unexpected experiment patch sequence')
    return patches


def valid_patch(patch):
    patch_path = patch.get('path') if isinstance(patch, dict) else None
    return (isinstance(patch_path, str) and '\\' not in patch_path
            and PurePosixPath(patch_path).parts[:1] == ('patches',)
            and len(PurePosixPath(patch_path).parts) == 2
            and '..' not in PurePosixPath(patch_path).parts
            and PurePosixPath(patch_path).as_posix() == patch_path
            and patch_path.endswith('.patch') and is_sha(patch.get('sha256')))


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
    crate_name = 'egui' if name == 'egui-touch-input' else 'eframe'
    prefix = crate_name + '-0.36.2'
    if (not isinstance(crate, dict) or crate.get('name') != crate_name
            or crate.get('version') != '0.36.2' or crate.get('prefix') != prefix
            or crate.get('vendorPath') != 'vendor/' + prefix
            or crate.get('changedFiles') != EXPERIMENT_FILES[name]):
        raise ValueError('Unexpected experiment dependency')
    sources = data.get('sourceFiles')
    if (not isinstance(sources, list)
            or any(not isinstance(item, dict) or set(item) != {'path', 'before', 'after'}
                   or not is_sha(item['before']) or not is_sha(item['after']) for item in sources)
            or [item['path'] for item in sources] != EXPERIMENT_SOURCE_FILES[name]):
        raise ValueError('Unexpected experiment source files')
    if (not all(valid_patch(patch) for patch in experiment_patches(data))
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
    patches = experiment_patches(metadata)
    # Check every part before applying the first. Ordering is a recipe contract,
    # not inferred from the directory or whichever patches happen to apply.
    for item in patches:
        if not valid_patch(item):
            raise ValueError('Unsafe experiment patch path')
        checked(package_root / item['path'], item['sha256'])
    for item in patches:
        apply_patch(source, package_root / item['path'])
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
    patch_proof = {'patch': metadata['patch']} if 'patch' in metadata else {'patches': patches}
    return {'name': metadata['name'], 'status': metadata['status'],
            'crate': crate, **patch_proof,
            'managedCargoLockSha256': metadata['managedCargoLockSha256'],
            'verifiedBeforeTree': before, 'verifiedAfterTree': after}
