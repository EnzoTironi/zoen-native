"""Exact egui touch-fix staging; no Cargo, browser, services or device actions."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('dependency_experiment', ROOT / 'dependency_experiment.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
UPSTREAM = json.loads((ROOT / 'upstream.json').read_text())
INPUT = module.load_experiment(ROOT, 'eframe-ios-input', UPSTREAM)
BASE = dict(UPSTREAM, managedCargoLockSha256=INPUT['managedCargoLockSha256'])
METADATA = module.load_experiment(ROOT, 'egui-touch-input', BASE)
BASELINE = Path(os.environ['ZOEN_DECKCRAFT_INPUT_BASELINE']).resolve(strict=True)
CRATE = Path(os.environ['ZOEN_EGUI_TEST_CRATE']).resolve(strict=True)


def apply_patch(source, patch):
    for extra in [['--dry-run'], []]:
        result = subprocess.run(['patch', '--batch', '--forward', '--fuzz=0', '-p1', '-i', str(patch)] + extra,
                                cwd=source, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        if result.returncode:
            raise AssertionError(result.stdout.decode())


class TouchInputDependencyTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='zoen-egui-touch-stage-')
        self.source = Path(self.temp.name) / 'source'
        self.source.mkdir()
        for item in METADATA['sourceFiles']:
            path = self.source / item['path']
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(BASELINE / item['path'], path)

    def tearDown(self):
        self.temp.cleanup()

    def test_exact_published_egui_archive_and_patch_stage_only_reviewed_source(self):
        archive_hash = module.digest(CRATE.read_bytes())
        proof = module.stage_experiment(self.source, METADATA, CRATE, ROOT, apply_patch)
        self.assertEqual(len(proof['verifiedBeforeTree']), 119)
        self.assertEqual(len(proof['verifiedAfterTree']), 120)
        before = {row['path']: row for row in proof['verifiedBeforeTree']}
        after = {row['path']: row for row in proof['verifiedAfterTree']}
        changed = sorted(name for name in set(before) | set(after) if before.get(name) != after.get(name))
        self.assertEqual(changed, METADATA['crate']['changedFiles'])
        for item in METADATA['sourceFiles']:
            self.assertEqual(module.digest((self.source / item['path']).read_bytes()), item['after'])
            self.assertEqual(module.digest((BASELINE / item['path']).read_bytes()), item['before'])
        self.assertEqual(module.digest(CRATE.read_bytes()), archive_hash)

    def test_notes_contract_hash_is_checked_before_first_touch_patch(self):
        metadata = copy.deepcopy(METADATA)
        metadata['patches'][1]['sha256'] = '0' * 64
        applied = []
        def record(source, patch):
            applied.append(patch)
            apply_patch(source, patch)
        with self.assertRaisesRegex(ValueError, 'input hash mismatch'):
            module.stage_experiment(self.source, metadata, CRATE, ROOT, record)
        self.assertEqual(applied, [], 'an invalid Notes patch must not partially apply touch changes')
        for item in metadata['sourceFiles']:
            self.assertEqual(module.digest((self.source / item['path']).read_bytes()), item['before'])

    def test_egui_stage_requires_exact_preceding_input_stage(self):
        with self.assertRaisesRegex(ValueError, 'Experiment base changed'):
            module.load_experiment(ROOT, 'egui-touch-input', UPSTREAM)
        (self.source / 'Cargo.lock').write_text('wrong prior stage')
        with self.assertRaisesRegex(ValueError, 'input hash mismatch'):
            module.stage_experiment(self.source, METADATA, CRATE, ROOT, apply_patch)
        self.assertFalse((self.source / 'vendor').exists())

    def test_unreviewed_dependency_file_is_refused_after_exact_patch(self):
        def extra_file(source, patch):
            apply_patch(source, patch)
            (source / 'vendor/egui-0.36.2/extra.rs').write_text('// outside reviewed scope\n')
        with self.assertRaisesRegex(ValueError, 'Unexpected patched dependency tree'):
            module.stage_experiment(self.source, METADATA, CRATE, ROOT, extra_file)

    def test_cli_refuses_unreviewed_order_duplicates_and_unpaired_archives_before_writes(self):
        for names, archives in [
            (['egui-touch-input'], 1),
            (['egui-touch-input', 'eframe-ios-input'], 2),
            (['eframe-ios-input', 'eframe-ios-input'], 2),
            (['eframe-ios-input', 'egui-touch-input'], 1),
        ]:
            work = Path(self.temp.name) / 'never-created'
            command = ['python3', '-B', str(ROOT / 'build.py'), '--upstream', '/not-used',
                       '--fonts', '/not-used', '--work', str(work)]
            for name in names:
                command += ['--experiment', name]
            for _ in range(archives):
                command += ['--experiment-crate', '/not-used']
            result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            with self.subTest(names=names, archives=archives):
                self.assertNotEqual(result.returncode, 0)
                self.assertRegex(result.stdout.decode(), 'Unreviewed dependency experiment chain|supplied together')
                self.assertFalse(work.exists())


if __name__ == '__main__':
    unittest.main()
