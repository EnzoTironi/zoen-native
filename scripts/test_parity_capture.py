#!/usr/bin/env python3
"""Capture orchestration safety checks; never contact an actual device."""
from datetime import datetime
import os
from pathlib import Path
import time
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import parity_capture


class CaptureSafetyTests(unittest.TestCase):
    def test_ios_status_time_is_iso_with_milliseconds_and_local_noon(self):
        # New runtimes reject HH:MM and ISO timestamps without milliseconds.
        # The offset keeps noon unchanged on runners in different time zones.
        try:
            for zone in ('UTC', 'America/Sao_Paulo'):
                with self.subTest(zone=zone), patch.dict(os.environ, TZ=zone):
                    time.tzset()
                    value = parity_capture.ios_status_time()
                    self.assertRegex(value, r'^2026-01-01T12:00:00\.000[+-]\d{2}:\d{2}$')
                    self.assertEqual(datetime.fromisoformat(value).astimezone().hour, 12)
        finally:
            # patch.dict has restored TZ, including on assertion failures.
            time.tzset()

    def test_parity_workflow_installs_wasm_target_before_web_build(self):
        workflow = (parity_capture.ROOT / '.github/workflows/ios-android-parity.yml').read_text()
        web = workflow.split('\n  web:\n', 1)[1].split('\n  compare:\n', 1)[0]
        self.assertIn('targets: wasm32-unknown-unknown', web)
        self.assertLess(web.index('targets: wasm32-unknown-unknown'), web.index('npm run build:web'))
        self.assertLess(web.index('cargo install wasm-bindgen-cli --version 0.2.129 --locked'),
                        web.index('npm run build:web'))

    def test_parity_android_sdk_setup_avoids_obsolete_tools(self):
        workflow = (parity_capture.ROOT / '.github/workflows/ios-android-parity.yml').read_text()
        android = workflow.split('\n  android:\n', 1)[1].split('\n  fixtures:\n', 1)[0]
        setup = android.split('uses: android-actions/setup-android@v3', 1)[1].split('      - ', 1)[0]
        self.assertIn('packages: platform-tools', setup)

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
