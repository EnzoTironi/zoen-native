#!/usr/bin/env python3
"""Capture orchestration safety checks; never contact an actual device."""
from datetime import datetime
import os
from pathlib import Path
import subprocess
import time
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import call, patch

import parity_capture


class CaptureSafetyTests(unittest.TestCase):
    def test_ios_core_build_uses_only_the_required_simulator_slice(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            library = root / 'target/aarch64-apple-ios-sim/release'
            library.mkdir(parents=True)
            (library / 'libroda_ffi.a').write_bytes(b'archive')
            (library / 'libroda_ffi.dylib').write_bytes(b'dylib')
            (root / 'apple/Packages/RodaCore/Sources/RodaCore').mkdir(parents=True)
            commands = []

            def fake_run(*args, **kwargs):
                commands.append((args, kwargs))
                if args[:2] == ('cargo', 'run'):
                    bindings = root / 'build/bindings'
                    bindings.mkdir(parents=True, exist_ok=True)
                    (bindings / 'roda_ffi.swift').write_text('swift')
                    (bindings / 'roda_ffiFFI.h').write_text('header')
                    (bindings / 'roda_ffiFFI.modulemap').write_text('module')
                elif args[:2] == ('xcodebuild', '-create-xcframework'):
                    Path(args[args.index('-output') + 1]).mkdir(parents=True)

            with patch.object(parity_capture, 'ROOT', root), \
                    patch.object(parity_capture, 'run', side_effect=fake_run), \
                    patch.object(parity_capture.subprocess, 'check_output', return_value='/Simulator.sdk\n'), \
                    patch.dict(os.environ, {'PROFILE': 'release', 'MACOSX_DEPLOYMENT_TARGET': '99.0'}):
                parity_capture.build_ios_core()

            build = next(entry for entry in commands if entry[0][:2] == ('cargo', 'build'))
            self.assertIn('aarch64-apple-ios-sim', build[0])
            self.assertNotIn('aarch64-apple-ios', build[0])
            self.assertNotIn('aarch64-apple-darwin', build[0])
            self.assertEqual(build[1]['env']['SDKROOT'], '/Simulator.sdk')
            self.assertNotIn('MACOSX_DEPLOYMENT_TARGET', build[1]['env'])
            framework = next(entry[0] for entry in commands if entry[0][:2] == ('xcodebuild', '-create-xcframework'))
            self.assertEqual(framework.count('-library'), 1)
            self.assertIn(str(library / 'libroda_ffi.a'), framework)
            self.assertEqual((root / 'apple/Packages/RodaCore/Sources/RodaCore/RodaFFI.generated.swift').read_text(), 'swift')

    def test_ios_workflow_installs_only_the_simulator_rust_target(self):
        workflow = (parity_capture.ROOT / '.github/workflows/ios-android-parity.yml').read_text()
        ios = workflow.split('\n  ios:\n', 1)[1].split('\n  android:\n', 1)[0]
        target_line = next(line.strip() for line in ios.splitlines() if line.strip().startswith('targets:'))
        self.assertEqual(target_line, 'targets: aarch64-apple-ios-sim')

    def test_ios_rust_cache_is_keyed_by_current_sources(self):
        workflow = (parity_capture.ROOT / '.github/workflows/ios-android-parity.yml').read_text()
        ios = workflow.split('\n  ios:\n', 1)[1].split('\n  android:\n', 1)[0]
        self.assertIn(
            "key: parity-ios-${{ hashFiles('Cargo.lock', 'Cargo.toml', "
            "'crates/**/Cargo.toml', 'crates/**/*.rs') }}",
            ios,
        )

    def test_ios_capture_uses_current_home_routes_for_community_and_notifications(self):
        source = (parity_capture.ROOT / 'apple/UITests/AndroidParityCaptureTests.swift').read_text()
        self.assertNotIn('["-RodaTab", "files"]', source)
        self.assertNotIn('["-RodaTab", "activity"]', source)
        self.assertIn('"Coastal Travelers"', source)
        self.assertIn('"home-notifications-card"', source)
        self.assertIn('"notifications-list-view"', source)

    def test_ios_capture_waits_for_current_search_controls(self):
        screens = (parity_capture.ROOT / 'apple/UITests/AndroidParityCaptureTests.swift').read_text()
        composer = (parity_capture.ROOT / 'apple/UITests/BottomComposerParityCaptureTests.swift').read_text()
        self.assertNotIn('buttons["New message"]', screens)
        self.assertNotIn('buttons["New message"]', composer)
        self.assertIn('app.buttons["Search"]', screens)
        self.assertIn('bottomBar.buttons["Search"]', screens)
        self.assertIn('bottomBar.buttons["Search"]', composer)
        self.assertIn('textFields["unified-search-field"]', composer)
        self.assertNotIn('navigationBars["New chat"]', composer)

    def test_ios_workflow_preserves_capture_log_without_precreating_output(self):
        workflow = (parity_capture.ROOT / '.github/workflows/ios-android-parity.yml').read_text()
        ios = workflow.split('\n  ios:\n', 1)[1].split('\n  android:\n', 1)[0]
        self.assertIn('mkdir -p parity', ios)
        self.assertNotIn('mkdir -p parity/ios', ios)
        self.assertIn('set -o pipefail', ios)
        self.assertIn('2>&1 | tee "parity/ios-${{ matrix.viewport }}.log"', ios)
        self.assertGreaterEqual(ios.count('parity/ios-${{ matrix.viewport }}.log'), 2)

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

    def test_web_capture_creates_parent_but_rejects_stale_output(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            playwright = root / 'fake-playwright.mjs'
            playwright.write_text(
                "export const webkit = { launch() { throw new Error('browser launch reached'); } };\n"
            )
            output = root / 'missing-parent' / 'web'
            command = (
                'node', str(parity_capture.ROOT / 'scripts/parity_web_capture.mjs'),
                '--url', 'http://127.0.0.1:1/', '--output', str(output),
                '--playwright', str(playwright), '--viewport', 'narrow',
            )

            first = subprocess.run(command, cwd=parity_capture.ROOT, capture_output=True, text=True)
            self.assertNotEqual(first.returncode, 0)
            self.assertIn('browser launch reached', first.stderr)
            self.assertTrue(output.is_dir())

            second = subprocess.run(command, cwd=parity_capture.ROOT, capture_output=True, text=True)
            self.assertNotEqual(second.returncode, 0)
            self.assertIn('EEXIST', second.stderr)
            self.assertNotIn('browser launch reached', second.stderr)

    def test_android_timezone_uses_supported_alarm_shell_command(self):
        adb = ('adb', '-s', 'emulator-5554')
        with patch.object(parity_capture, 'run') as run:
            parity_capture.set_android_timezone(adb)
        run.assert_called_once_with(*adb, 'shell', 'cmd', 'alarm', 'set-timezone', 'UTC')
        self.assertNotIn('setprop', run.call_args.args)

    def test_android_file_read_waits_for_same_device_after_offline_transport(self):
        adb = ('adb', '-s', 'emulator-5554')
        path = 'cache/device-evidence/android-parity/chats-light.png'
        read = [*adb, 'exec-out', 'run-as', 'xyz.tironi.zoen', 'cat', path]
        wait = [*adb, 'wait-for-device']
        offline = subprocess.CompletedProcess(read, 1, b'', b'error: device offline\n')
        ready = subprocess.CompletedProcess(wait, 0, b'', b'')
        success = subprocess.CompletedProcess(read, 0, b'fresh-png', b'')

        with patch.object(parity_capture.subprocess, 'run', side_effect=(offline, ready, success)) as run:
            self.assertEqual(parity_capture.read_android_file(adb, path), b'fresh-png')

        self.assertEqual(run.call_args_list, [
            call(read, cwd=parity_capture.ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE),
            call(wait, check=True, cwd=parity_capture.ROOT, stdout=subprocess.DEVNULL,
                 stderr=subprocess.PIPE, timeout=30),
            call(read, cwd=parity_capture.ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE),
        ])

    def test_android_file_read_does_not_retry_missing_file(self):
        adb = ('adb', '-s', 'emulator-5554')
        path = 'cache/device-evidence/android-parity/missing.png'
        read = [*adb, 'exec-out', 'run-as', 'xyz.tironi.zoen', 'cat', path]
        missing = subprocess.CompletedProcess(read, 1, b'', b'cat: missing.png: No such file or directory\n')

        with patch.object(parity_capture.subprocess, 'run', return_value=missing) as run, \
                self.assertRaises(subprocess.CalledProcessError):
            parity_capture.read_android_file(adb, path)

        run.assert_called_once_with(read, cwd=parity_capture.ROOT,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE)

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
