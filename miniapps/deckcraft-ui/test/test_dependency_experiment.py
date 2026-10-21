"""Source-preparation checks only; no Cargo, Git, browser or native build."""
import copy
import difflib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('dependency_experiment', ROOT / 'dependency_experiment.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
BASELINE = Path(os.environ['ZOEN_DECKCRAFT_TEST_BASELINE']).resolve(strict=True)
CRATE = Path(os.environ['ZOEN_EFRAME_TEST_CRATE']).resolve(strict=True)
UPSTREAM = json.loads((ROOT / 'upstream.json').read_text())
METADATA = module.load_experiment(ROOT, 'eframe-shift', UPSTREAM)


def apply_patch(source, patch):
    # The builder retains its original checked Git application. This fixture
    # exercises the same unified diff with the installed patch utility, never Git.
    for extra in [['--dry-run'], []]:
        result = subprocess.run(['patch', '--batch', '--forward', '--fuzz=0', '-p1', '-i', str(patch)] + extra,
                                cwd=source, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        if result.returncode:
            raise AssertionError(result.stdout.decode())


class DependencyExperimentTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='zoen-eframe-stage-test-')
        self.source = Path(self.temp.name) / 'source'
        self.source.mkdir()
        for name in ['Cargo.toml', 'Cargo.lock']:
            shutil.copy2(BASELINE / name, self.source / name)

    def tearDown(self):
        self.temp.cleanup()

    def test_actual_archive_patch_and_exact_tree_are_isolated(self):
        before_archive = module.digest(CRATE.read_bytes())
        proof = module.stage_experiment(self.source, METADATA, CRATE, ROOT, apply_patch)
        self.assertEqual(proof['managedCargoLockSha256'], METADATA['managedCargoLockSha256'])
        self.assertEqual(len(proof['verifiedBeforeTree']), 35)
        self.assertEqual(len(proof['verifiedAfterTree']), 35)
        before = {item['path']: item for item in proof['verifiedBeforeTree']}
        after = {item['path']: item for item in proof['verifiedAfterTree']}
        self.assertEqual([name for name in before if before[name] != after[name]], ['src/web/text_agent.rs'])
        self.assertEqual(module.digest(CRATE.read_bytes()), before_archive)
        self.assertEqual(module.digest((BASELINE / 'Cargo.lock').read_bytes()), UPSTREAM['managedCargoLockSha256'])

    def test_wrong_complete_archive_hash_refuses_before_vendor_creation(self):
        bad = Path(self.temp.name) / 'bad.crate'
        bad.write_bytes(CRATE.read_bytes() + b'changed')
        with self.assertRaisesRegex(ValueError, 'input hash mismatch'):
            module.stage_experiment(self.source, METADATA, bad, ROOT, apply_patch)
        self.assertFalse((self.source / 'vendor').exists())

    def test_unsafe_archive_member_is_rejected_even_with_matching_test_digest(self):
        bad = Path(self.temp.name) / 'unsafe.crate'
        with tarfile.open(bad, 'w:gz') as tar:
            entry = tarfile.TarInfo('eframe-0.36.2/../escape')
            entry.size = 1
            tar.addfile(entry, io.BytesIO(b'x'))
        metadata = copy.deepcopy(METADATA)
        metadata['crate']['archiveSha256'] = module.digest(bad.read_bytes())
        with self.assertRaisesRegex(ValueError, 'Unsafe dependency archive member'):
            module.stage_experiment(self.source, metadata, bad, ROOT, apply_patch)
        self.assertFalse((self.source / 'vendor/escape').exists())

    def test_wrong_base_refuses_before_any_dependency_staging(self):
        (self.source / 'Cargo.lock').write_text('different accepted base')
        with self.assertRaisesRegex(ValueError, 'input hash mismatch'):
            module.stage_experiment(self.source, METADATA, CRATE, ROOT, apply_patch)
        self.assertFalse((self.source / 'vendor').exists())

    def test_patch_hash_and_unexpected_after_tree_fail_closed(self):
        metadata = copy.deepcopy(METADATA)
        metadata['patch']['sha256'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'input hash mismatch'):
            module.stage_experiment(self.source, metadata, CRATE, ROOT, apply_patch)
        self.assertEqual(module.digest((self.source / 'Cargo.lock').read_bytes()), UPSTREAM['managedCargoLockSha256'])
        shutil.rmtree(self.source / 'vendor')
        def mutate_extra(source, patch):
            apply_patch(source, patch)
            (source / 'vendor/eframe-0.36.2/unexpected.txt').write_text('unreviewed')
        with self.assertRaisesRegex(ValueError, 'Unexpected patched dependency tree'):
            module.stage_experiment(self.source, METADATA, CRATE, ROOT, mutate_extra)

    def test_metadata_base_and_explicit_cli_pair_are_enforced(self):
        wrong = dict(UPSTREAM, revision='0' * 40)
        with self.assertRaisesRegex(ValueError, 'Experiment base changed'):
            module.load_experiment(ROOT, 'eframe-shift', wrong)
        with self.assertRaisesRegex(ValueError, 'Unknown dependency experiment'):
            module.load_experiment(ROOT, 'not-a-default', UPSTREAM)
        work = Path(self.temp.name) / 'not-created'
        result = subprocess.run(['python3', str(ROOT / 'build.py'), '--upstream', '/not-used', '--fonts', '/not-used',
                                 '--work', str(work), '--experiment', 'eframe-shift'],
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b'supplied together', result.stdout)
        self.assertFalse(work.exists())


    def input_metadata_fixture(self):
        # Admission-only fixture, not final ios-input release hashes/metadata.
        data = copy.deepcopy(METADATA)
        data['name'] = 'eframe-ios-input'
        data['crate']['changedFiles'] = [
            'src/web/app_runner.rs', 'src/web/backend.rs', 'src/web/events.rs',
            'src/web/mod.rs', 'src/web/text_agent.rs', 'src/web/text_input_activation.rs',
        ]
        data['sourceFiles'] += [
            {'path': name, 'before': '1' * 64, 'after': '2' * 64}
            for name in ['crates/ui-egui/src/canvas.rs', 'crates/ui-egui/src/host_phone_landscape_tests.rs']
        ]
        return data

    def load_metadata_fixture(self, data, name='eframe-ios-input'):
        package = Path(self.temp.name) / 'metadata-package'
        (package / 'experiments').mkdir(parents=True, exist_ok=True)
        (package / 'experiments' / (name + '.json')).write_text(json.dumps(data))
        return module.load_experiment(package, name, UPSTREAM)

    def test_explicit_input_name_keeps_the_shift_recipe_exact_and_file_scopes_separate(self):
        self.assertEqual(module.load_experiment(ROOT, 'eframe-shift', UPSTREAM), METADATA)
        data = self.input_metadata_fixture()
        self.assertEqual(self.load_metadata_fixture(data), data)
        for name, files in [
            ('eframe-ios-input', ['src/web/text_agent.rs']),
            ('eframe-shift', data['crate']['changedFiles']),
            ('eframe-ios-input', data['crate']['changedFiles'] + ['src/web/unreviewed.rs']),
        ]:
            bad = copy.deepcopy(data if name == 'eframe-ios-input' else METADATA)
            bad['crate']['changedFiles'] = files
            with self.subTest(name=name, files=files), self.assertRaisesRegex(ValueError, 'Unexpected experiment dependency'):
                self.load_metadata_fixture(bad, name)
        self.assertFalse((self.source / 'vendor').exists())

    def test_malformed_input_metadata_refuses_before_staging(self):
        changes = [
            lambda d: d.update(schema=True),
            lambda d: d.update(crate=None),
            lambda d: d['crate'].update(vendorPath='vendor/other'),
            lambda d: d['crate'].update(archiveSha256=None),
            lambda d: d['crate'].update(beforeTreeSha256='not-a-hash'),
            lambda d: d['crate'].update(sourceRevision='0' * 39),
            lambda d: d.update(sourceFiles=d['sourceFiles'][:2]),
            lambda d: d['sourceFiles'][2].update(path='../escape'),
            lambda d: d['sourceFiles'][3].update(path=d['sourceFiles'][2]['path']),
            lambda d: d['sourceFiles'][2].update(before=None),
            lambda d: d['sourceFiles'][1].update(after='f' * 64),
            lambda d: d.update(patch=None),
            lambda d: d['patch'].update(path='../outside.patch'),
            lambda d: d['patch'].update(path='patches/../outside.patch'),
            lambda d: d['patch'].update(sha256='short'),
        ]
        for index, change in enumerate(changes):
            data = self.input_metadata_fixture()
            change(data)
            with self.subTest(index=index), self.assertRaises(ValueError):
                self.load_metadata_fixture(data)
        self.assertFalse((self.source / 'vendor').exists())

    def test_input_archive_stages_only_six_vendor_files_and_four_exact_source_hashes(self):
        # The actual pinned .crate is used, but edits are bounded comment-only
        # fixtures, not a substitute for the pending compiled first-touch patch.
        data = self.input_metadata_fixture()
        names = ['Cargo.toml', 'Cargo.lock', 'crates/ui-egui/src/canvas.rs',
                 'crates/ui-egui/src/host_phone_landscape_tests.rs']
        for name in names[2:]:
            path = self.source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b'// fixture original\n')
        source_before = {name: (self.source / name).read_bytes() for name in names}
        source_after = {name: value + b'\n// fixture source edit\n' for name, value in source_before.items()}
        # Cargo inputs stay valid TOML fixtures; no Cargo is run here.
        for name in names[:2]:
            source_after[name] = source_before[name] + b'\n# fixture source edit\n'
        with tarfile.open(CRATE, 'r:gz') as tar:
            original = {str(Path(member.name).relative_to('eframe-0.36.2')): tar.extractfile(member).read()
                        for member in tar.getmembers() if member.isfile()}
        changed = data['crate']['changedFiles']
        after = dict(original)
        for name in changed:
            after[name] = original.get(name, b'') + b'\n// fixture vendor edit\n'
        rows = [{'path': name, 'bytes': len(value), 'sha256': module.digest(value)}
                for name, value in sorted(after.items())]
        data['crate']['afterTreeSha256'] = module.tree_digest(rows)
        data['sourceFiles'] = [{'path': name, 'before': module.digest(source_before[name]),
                                'after': module.digest(source_after[name])} for name in names]
        data['managedCargoLockSha256'] = module.digest(source_after['Cargo.lock'])
        patch = ''
        for name in names:
            patch += ''.join(difflib.unified_diff(source_before[name].decode().splitlines(keepends=True),
                                                source_after[name].decode().splitlines(keepends=True),
                                                fromfile='a/' + name, tofile='b/' + name))
        for name in changed:
            path = 'vendor/eframe-0.36.2/' + name
            patch += ''.join(difflib.unified_diff(original.get(name, b'').decode().splitlines(keepends=True),
                                                after[name].decode().splitlines(keepends=True),
                                                fromfile='a/' + path if name in original else '/dev/null',
                                                tofile='b/' + path))
        package = Path(self.temp.name) / 'stage-package'
        (package / 'patches').mkdir(parents=True)
        patch_path = package / 'patches' / 'fixture-ios-input.patch'
        patch_path.write_text(patch)
        data['patch'] = {'path': 'patches/fixture-ios-input.patch', 'sha256': module.digest(patch.encode())}
        (package / 'experiments').mkdir()
        (package / 'experiments' / 'eframe-ios-input.json').write_text(json.dumps(data))
        loaded = module.load_experiment(package, 'eframe-ios-input', UPSTREAM)
        proof = module.stage_experiment(self.source, loaded, CRATE, package, apply_patch)
        self.assertEqual(len(proof['verifiedBeforeTree']), 35)
        self.assertEqual(len(proof['verifiedAfterTree']), 36)
        before_by_path = {row['path']: row for row in proof['verifiedBeforeTree']}
        after_by_path = {row['path']: row for row in proof['verifiedAfterTree']}
        self.assertEqual(sorted(name for name in set(before_by_path) | set(after_by_path)
                                if before_by_path.get(name) != after_by_path.get(name)), changed)
        for name in names:
            self.assertEqual((self.source / name).read_bytes(), source_after[name])

    def test_new_cli_name_requires_its_exact_local_crate_and_unknown_names_stay_closed(self):
        for name, expected in [('eframe-ios-input', b'supplied together'),
                               ('unreviewed-input', b'invalid choice')]:
            work = Path(self.temp.name) / name
            result = subprocess.run(['python3', str(ROOT / 'build.py'), '--upstream', '/not-used',
                                     '--fonts', '/not-used', '--work', str(work), '--experiment', name],
                                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(expected, result.stdout)
            self.assertFalse(work.exists())


if __name__ == '__main__':
    unittest.main()
