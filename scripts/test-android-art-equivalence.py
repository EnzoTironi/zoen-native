#!/usr/bin/env python3
"""Pet source gate regression: UI chrome is not art, sprite changes are."""
import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('upstream', ROOT / 'scripts/check-android-upstream.py')
upstream = importlib.util.module_from_spec(spec)
spec.loader.exec_module(upstream)
PET = 'apple/Shared/Apps/Native/PetViews.swift'


class ArtEquivalenceTests(unittest.TestCase):
    def verify(self, transform):
        run = subprocess.run

        def changed_main(command, **kwargs):
            result = run(command, **kwargs)
            if command[:2] == ['git', 'show'] and command[2].endswith(':' + PET):
                result.stdout = transform(result.stdout)
            return result

        with patch.object(upstream.subprocess, 'run', side_effect=changed_main):
            return upstream.artwork(ROOT, 'origin/main')

    def test_current_sources_and_generated_output_match(self):
        art = self.verify(lambda source: source)
        self.assertTrue(all(value['matches'] for value in art['original_sources'].values()))
        self.assertTrue(art['generated_pet_matches_original_source'])

    def test_ui_only_change_preserves_art(self):
        art = self.verify(lambda source: source + b'\n// UI chrome only\n')
        self.assertTrue(art['original_sources'][PET]['matches'])

    def test_changed_sprite_is_rejected(self):
        art = self.verify(lambda source: b'// changed sprite inputs\n' + source)
        self.assertFalse(art['original_sources'][PET]['matches'])

    def test_missing_sprite_boundary_is_rejected(self):
        art = self.verify(lambda source: source.replace(b'struct PetSprite:', b'struct RenamedSprite:'))
        self.assertFalse(art['original_sources'][PET]['matches'])


if __name__ == '__main__':
    unittest.main()
