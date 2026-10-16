#!/usr/bin/env python3
"""Capture orchestration safety checks; never contact an actual device."""
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import parity_capture


class CaptureSafetyTests(unittest.TestCase):
    def test_capture_rejects_physical_phone_before_any_command(self):
        with TemporaryDirectory() as directory, patch.object(parity_capture, 'run') as run:
            with self.assertRaisesRegex(ValueError, 'only supports emulator'):
                parity_capture.capture_android(SimpleNamespace(device='physical-phone'), Path(directory))
            run.assert_not_called()

    def test_only_online_emulator_is_selected_not_connected_phone(self):
        listing = 'List of devices attached\nphysical-phone\tdevice\nemulator-5554\tdevice\nemulator-5556\toffline\n'
        with patch.object(parity_capture.subprocess, 'check_output', return_value=listing):
            self.assertEqual(parity_capture.android_emulator_serial(), 'emulator-5554')

    def test_multiple_emulators_require_explicit_selection(self):
        listing = 'emulator-5554\tdevice\nemulator-5556\tdevice\n'
        with patch.object(parity_capture.subprocess, 'check_output', return_value=listing):
            with self.assertRaisesRegex(ValueError, 'Supply --device'):
                parity_capture.android_emulator_serial()
            self.assertEqual(parity_capture.android_emulator_serial('emulator-5556'), 'emulator-5556')

    def test_unavailable_emulator_is_rejected(self):
        with patch.object(parity_capture.subprocess, 'check_output', return_value='physical-phone\tdevice\n'):
            with self.assertRaisesRegex(ValueError, 'not online'):
                parity_capture.android_emulator_serial('emulator-5554')

    def test_capture_selects_exactly_the_sixteen_required_cases(self):
        cases = parity_capture.android_capture_test_classes().split(',')
        self.assertEqual(len(cases), 16)
        self.assertEqual(len(set(cases)), 16)
        self.assertIn('xyz.tironi.zoen.AndroidParityComposerCaptureTest#composerLight', cases)
        self.assertIn('xyz.tironi.zoen.AndroidParityScreensTest#spacesDark', cases)
        self.assertIn('xyz.tironi.zoen.AndroidParityScreensTest#notificationsLight', cases)
        self.assertIn('xyz.tironi.zoen.AndroidParityScreensTest#profileDark', cases)
        self.assertFalse(any('#thread' in case for case in cases))

    def test_instrumentation_requires_actual_success_marker(self):
        parity_capture.validate_instrumentation_output('OK (12 tests)\nINSTRUMENTATION_CODE: -1')
        for output in ('', 'INSTRUMENTATION_FAILED: runner', 'FAILURES!!!\nOK (1 test)',
                       'Process crashed.\nOK (1 test)', 'shortMsg=timeout\nOK (1 test)'):
            with self.subTest(output=output), self.assertRaises(ValueError):
                parity_capture.validate_instrumentation_output(output)


if __name__ == '__main__':
    unittest.main()
