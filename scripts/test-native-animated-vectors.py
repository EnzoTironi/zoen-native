#!/usr/bin/env python3
"""Resource contract, supplemented by NativeAnimatedDrawableTest on Android."""
from pathlib import Path
import importlib.util
import re
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
A = '{http://schemas.android.com/apk/res/android}'
spec = importlib.util.spec_from_file_location('generator', ROOT / 'scripts/generate-native-animated-vectors.py')
generator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generator)

class NativeVectors(unittest.TestCase):
    def test_every_chrome_glyph_has_a_native_resource(self):
        source = (ROOT / 'android/app/src/main/java/xyz/tironi/zoen/ui/ink/GeneratedInkIcons.kt').read_text()
        names = re.search(r'enum class ZoenGlyph \{([^}]+)', source).group(1)
        for name in names.split(','):
            self.assertIn(name.strip().lower(), generator.GLYPHS)

    def test_effects_target_the_drawable_not_the_control(self):
        for name in generator.GLYPHS:
            resource = generator.ROOT / f'native_{name}.xml'
            static = ET.parse(resource).getroot()
            self.assertEqual(static.tag, 'vector')
            self.assertEqual(static.find('group').get(A+'name'), 'symbol')
            for effect in ('bounce','wiggle','active','draw'):
                animated = ET.parse(generator.ROOT / f'native_{name}_{effect}.xml').getroot()
                self.assertEqual(animated.tag, 'animated-vector')
                self.assertEqual(animated.get(A+'drawable'), f'@drawable/native_{name}')
                animators = list(animated.iter('objectAnimator'))
                self.assertTrue(animators)
                self.assertTrue(all(int(a.get(A+'duration')) > 0 for a in animators))
                self.assertTrue(all(a.get(A+'repeatCount') is None for a in animators), 'Only live state may repeat')

    def test_release_shrinker_keeps_named_resources(self):
        self.assertIn('@drawable/native_*', (ROOT / 'android/app/src/main/res/raw/keep.xml').read_text())

if __name__ == '__main__':
    unittest.main()
