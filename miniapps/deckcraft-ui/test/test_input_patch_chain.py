"""The complete input patch family uses exact local archives; no compilation."""
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
METADATA = module.load_experiment(ROOT, 'eframe-ios-input', UPSTREAM)
BASELINE = Path(os.environ['ZOEN_DECKCRAFT_TEST_BASELINE']).resolve(strict=True)
CRATE = Path(os.environ['ZOEN_EFRAME_TEST_CRATE']).resolve(strict=True)


def apply_patch(source, patch):
    for command in [['git', 'apply', '--check', str(patch)], ['git', 'apply', str(patch)]]:
        subprocess.run(command, cwd=source, check=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)


class InputPatchChainTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='zoen-input-patch-chain-')
        self.source = Path(self.temp.name) / 'source'
        self.source.mkdir()
        for item in METADATA['sourceFiles']:
            path = self.source / item['path']
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(BASELINE / item['path'], path)

    def tearDown(self):
        self.temp.cleanup()

    def load(self, data):
        package = Path(self.temp.name) / 'metadata'
        (package / 'experiments').mkdir(parents=True, exist_ok=True)
        (package / 'experiments/eframe-ios-input.json').write_text(json.dumps(data))
        return module.load_experiment(package, 'eframe-ios-input', UPSTREAM)

    def test_complete_ordered_family_retains_all_original_source_and_vendor_hashes(self):
        archive_hash = module.digest(CRATE.read_bytes())
        applied = []

        def record(source, patch):
            applied.append(str(patch.relative_to(ROOT)))
            apply_patch(source, patch)

        proof = module.stage_experiment(self.source, METADATA, CRATE, ROOT, record)
        self.assertEqual(applied, list(module.IOS_INPUT_PATCHES))
        self.assertEqual(proof['patches'], METADATA['patches'])
        self.assertNotIn('patch', proof)
        self.assertEqual(len(proof['verifiedBeforeTree']), 35)
        self.assertEqual(len(proof['verifiedAfterTree']), 36)
        self.assertEqual(module.tree_digest(proof['verifiedAfterTree']), METADATA['crate']['afterTreeSha256'])
        for item in METADATA['sourceFiles']:
            self.assertEqual(module.digest((self.source / item['path']).read_bytes()), item['after'])
            self.assertEqual(module.digest((BASELINE / item['path']).read_bytes()), item['before'])
        self.assertEqual(module.digest(CRATE.read_bytes()), archive_hash)

    def test_wrong_order_missing_duplicate_ambiguous_and_unsafe_parts_refuse_before_staging(self):
        changes = [
            lambda d: d['patches'].reverse(),
            lambda d: d['patches'].pop(),
            lambda d: d['patches'].__setitem__(1, d['patches'][0]),
            lambda d: d.update(patch=d['patches'][0]),
            lambda d: d.update(patches=[]),
            lambda d: d.update(patches=None),
            lambda d: d['patches'].__setitem__(0, None),
            lambda d: d['patches'][0].update(path='patches/../outside.patch'),
            lambda d: d['patches'][1].update(sha256='short'),
        ]
        for index, change in enumerate(changes):
            data = copy.deepcopy(METADATA)
            change(data)
            with self.subTest(index=index), self.assertRaises(ValueError):
                self.load(data)
        self.assertFalse((self.source / 'vendor').exists())

    def test_every_part_hash_is_checked_before_the_first_application(self):
        data = copy.deepcopy(METADATA)
        data['patches'][-1]['sha256'] = '0' * 64
        applied = []
        with self.assertRaisesRegex(ValueError, 'input hash mismatch'):
            module.stage_experiment(self.source, data, CRATE, ROOT, lambda *args: applied.append(args))
        self.assertEqual(applied, [])
        for item in METADATA['sourceFiles']:
            self.assertEqual(module.digest((self.source / item['path']).read_bytes()), item['before'])

    def test_final_closed_vendor_tree_still_refuses_an_extra_file(self):
        def extra(source, patch):
            apply_patch(source, patch)
            if patch.name == Path(module.IOS_INPUT_PATCHES[-1]).name:
                (source / METADATA['crate']['vendorPath'] / 'extra.rs').write_text('// unreviewed\n')

        with self.assertRaisesRegex(ValueError, 'Unexpected patched dependency tree'):
            module.stage_experiment(self.source, METADATA, CRATE, ROOT, extra)


if __name__ == '__main__':
    unittest.main()
