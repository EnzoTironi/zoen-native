#!/usr/bin/env python3
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from PIL import Image
from parity_compare import KEYS, compare, normalize
from parity_capture import export_ios


class ParityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.ios, self.android = self.root/'ios', self.root/'android'
        self.config = {'size': [10, 10], 'ios': {'os_bar_crop': [0, 0, 0, 0]}, 'android': {'os_bar_crop': [0, 0, 0, 0]}}
        for folder in (self.ios, self.android):
            folder.mkdir()
            (folder/'manifest.json').write_text(json.dumps(dict(fixture_id='native-parity-v1', fixed_time='2026-01-15T12:00:00Z', locale='en_US', timezone='UTC', seed=128, source_revision='abc')))
            for key in KEYS:
                Image.new('RGB', (10, 10), 'white').save(folder/f'{key}.png')

    def compare(self, threshold=2.5):
        with contextlib.redirect_stdout(io.StringIO()):
            return compare(self.ios, self.android, self.root/'out', self.config, threshold)

    def test_identical_passes_and_emits_all_diffs(self):
        self.assertTrue(self.compare())
        self.assertEqual(len(list((self.root/'out').glob('*-diff.png'))), len(KEYS))

    def test_real_difference_fails_without_stopping_remaining_screens(self):
        image = Image.new('RGB', (10, 10), 'white')
        for x in range(3):
            image.putpixel((x, 0), (254, 255, 255))
        image.save(self.android/'chats-light.png')
        self.assertFalse(self.compare())
        report = json.loads((self.root/'out/report.json').read_text())
        self.assertEqual(len(report['screens']), len(KEYS))
        self.assertEqual(report['screens'][0]['different_pixels_percent'], 3.0)
        self.assertTrue(self.compare(3.0))

    def test_web_profile_reports_target_and_viewport(self):
        self.config['web'] = {'os_bar_crop': [0, 0, 0, 0]}
        self.config['viewport'] = 'wide-ipad-landscape'
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertTrue(compare(self.ios, self.android, self.root/'out', self.config, platform='web'))
        report = json.loads((self.root/'out/report.json').read_text())
        self.assertEqual(report['platform'], 'web')
        self.assertEqual(report['normalization']['viewport'], 'wide-ipad-landscape')
        self.assertTrue((self.root/'out/chats-light-web.png').exists())

    def test_wrong_but_matching_fixture_values_rejected(self):
        for folder in (self.ios, self.android):
            path = folder/'manifest.json'
            manifest = json.loads(path.read_text())
            manifest['seed'] = 999
            path.write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, 'Unsupported'):
            self.compare()

    def test_missing_frame_is_failure_with_other_percentages(self):
        (self.android/'chat-dark.png').unlink()
        self.assertFalse(self.compare())
        report = json.loads((self.root/'out/report.json').read_text())
        self.assertEqual(sum(row['different_pixels_percent'] is not None for row in report['screens']), len(KEYS) - 1)

    def test_missing_or_mismatched_fixture_rejected(self):
        (self.android/'manifest.json').write_text('{}')
        with self.assertRaisesRegex(ValueError, 'fixture'):
            self.compare()

    def test_declared_crop_only_and_no_aspect_distortion(self):
        image = Image.new('RGB', (20, 24), 'white')
        self.assertEqual(normalize(image, [0, 2, 0, 2], [10, 10]).size, (10, 10))
        with self.assertRaisesRegex(ValueError, 'aspect'):
            normalize(image, [0, 0, 0, 0], [10, 10])
        with self.assertRaises(ValueError):
            normalize(image, [-1, 0, 0, 0], [10, 10])

    def test_export_requires_all_fresh_native_attachments(self):
        exported = self.root/'exported'
        exported.mkdir()
        index = []
        for key in KEYS:
            filename = key+'.png'
            Image.new('RGB', (10, 10)).save(exported/filename)
            index.append(dict(exportedFileName=filename, suggestedHumanReadableName='ios-'+key+'_0_UUID.png'))
        (exported/'fixture.json').write_text((self.ios/'manifest.json').read_text())
        index.append(dict(exportedFileName='fixture.json', suggestedHumanReadableName='parity-fixture.json_0_UUID.json'))
        (exported/'manifest.json').write_text(json.dumps(index))
        output = self.root/'extracted'
        output.mkdir()
        export_ios(exported, output)
        self.assertEqual(len(list(output.glob('*.png'))), len(KEYS))
        index.pop(0)
        (exported/'manifest.json').write_text(json.dumps(index))
        with self.assertRaisesRegex(ValueError, 'Missing fresh'):
            export_ios(exported, output)

if __name__ == '__main__':
    unittest.main()
