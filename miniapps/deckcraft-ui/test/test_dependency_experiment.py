"""Source-preparation checks only; no Cargo, Git, browser or native build."""
import copy
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


if __name__ == '__main__':
    unittest.main()
