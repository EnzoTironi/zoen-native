"""The Notes contract patch is an exact ordered extension, never an arbitrary patch list."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('notes_dependency_experiment', ROOT / 'dependency_experiment.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class NotesRecipeTest(unittest.TestCase):
    def metadata(self):
        return json.loads((ROOT / 'experiments/egui-touch-input.json').read_text())

    def test_actual_notes_chain_preserves_base_and_each_patch_hash(self):
        upstream = json.loads((ROOT / 'upstream.json').read_text())
        eframe = module.load_experiment(ROOT, 'eframe-ios-input', upstream)
        base = dict(upstream, managedCargoLockSha256=eframe['managedCargoLockSha256'])
        data = module.load_experiment(ROOT, 'egui-touch-input', base)
        for patch in module.experiment_patches(data):
            module.checked(ROOT / patch['path'], patch['sha256'])
        self.assertEqual(data['baseManagedCargoLockSha256'], eframe['managedCargoLockSha256'])

    def test_notes_patch_cannot_reorder_duplicate_extend_or_switch_family(self):
        original = self.metadata()
        mutations = [
            lambda d: d['patches'].reverse(),
            lambda d: d['patches'].append(copy.deepcopy(d['patches'][0])),
            lambda d: d['patches'].pop(0),
            lambda d: d['patches'][1].update(path='patches/unreviewed.patch'),
            lambda d: d.update(name='eframe-shift'),
            lambda d: d.update(patch=copy.deepcopy(d['patches'][0])),
        ]
        for mutate in mutations:
            data = copy.deepcopy(original)
            mutate(data)
            with self.subTest(data=data):
                with self.assertRaisesRegex(ValueError, 'Unexpected experiment patch sequence'):
                    module.experiment_patches(data)

    def test_unchanged_single_patch_recipe_still_requires_its_checked_hash(self):
        upstream = json.loads((ROOT / 'upstream.json').read_text())
        legacy = module.load_experiment(ROOT, 'eframe-shift', upstream)
        patch = module.experiment_patches(legacy)[0]
        module.checked(ROOT / patch['path'], patch['sha256'])
        with self.assertRaisesRegex(ValueError, 'input hash mismatch'):
            module.checked(ROOT / patch['path'], '0' * 64)
