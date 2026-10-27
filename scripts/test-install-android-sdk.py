#!/usr/bin/env python3
"""Real subprocess fixtures; no SDK, network, compiler or device is used."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('sdk_install', Path(__file__).with_name('install-android-sdk.py'))
SDK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SDK)
PACKAGE = 'ndk;27.1.12297006'
FAILURE = 'Warning: An error occurred while preparing SDK package NDK (Side by side) 27.1.12297006: Error on ZipFile unknown archive.'
FAKE = r'''
import json,sys
from pathlib import Path
root=Path(__file__).parent
config=json.loads((root/'config.json').read_text())
countpath=root/'count'
n=int(countpath.read_text()) if countpath.exists() else 0
countpath.write_text(str(n+1))
sdk=Path(sys.argv[1].split('=',1)[1]);package=sys.argv[2]
assert package=='ndk;27.1.12297006'
target=sdk/'ndk/27.1.12297006';stage=sdk/'.temp/PackageOperation01'
if n and config.get('expectCleared'):
 assert stage.is_dir() and not list(stage.iterdir()), 'corrupt stage reused'
mode=config['results'][min(n,len(config['results'])-1)]
if mode=='success':
 target.mkdir(parents=True,exist_ok=True)
 (target/'package.xml').write_text('installed')
 print('Installed pinned NDK');sys.exit(0)
if mode=='corrupt':
 stage.mkdir(parents=True,exist_ok=True)
 (stage/'archive.zip').write_text('corrupt')
 (stage/'unzip').mkdir(exist_ok=True)
 metadata=target/'.installer';metadata.mkdir(parents=True,exist_ok=True)
 (metadata/'.installData').write_text('path='+str(stage)+'\nclass=com.android.repository.impl.installer.BasicInstaller\n')
 print(config['failure']);sys.exit(7)
print('Error: License not accepted');sys.exit(9)
'''


class InstallTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.sdk = self.root / 'sdk'
        self.sdk.mkdir()
        self.manager = self.root / 'sdkmanager'
        self.manager.write_text('#!' + sys.executable + '\n' + FAKE)
        self.manager.chmod(0o755)
        self.logs = self.root / 'logs'
        # An existing working package and an unrelated failed stage must survive.
        self.working = self.sdk / 'ndk/previous/source.properties'
        self.working.parent.mkdir(parents=True)
        self.working.write_text('working')
        self.foreign = self.sdk / '.temp/PackageOperation02/foreign.zip'
        self.foreign.parent.mkdir(parents=True)
        self.foreign.write_text('foreign')

    def run_installer(self, results, **extra):
        config = dict(results=results, failure=FAILURE, **extra)
        (self.root / 'config.json').write_text(json.dumps(config))
        command = [sys.executable, '-c',
                   'import importlib.util,sys; s=importlib.util.spec_from_file_location("sdk",sys.argv[1]); '
                   'm=importlib.util.module_from_spec(s);s.loader.exec_module(m); '
                   'raise SystemExit(m.install(sys.argv[2],sys.argv[3],sys.argv[4],("ndk;27.1.12297006",)))',
                   str(Path(SDK.__file__)), str(self.sdk), str(self.logs), str(self.manager)]
        result = subprocess.run(command, capture_output=True)
        self.assertEqual(self.working.read_text(), 'working')
        self.assertEqual(self.foreign.read_text(), 'foreign')
        return result

    def calls(self):
        return int((self.root / 'count').read_text())

    def test_success_runs_once(self):
        result = self.run_installer(['success'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), 1)
        self.assertEqual(len(list(self.logs.iterdir())), 1)

    def test_corrupt_archive_cleared_and_redownloaded(self):
        result = self.run_installer(['corrupt', 'success'], expectCleared=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), 2)
        logs = sorted(self.logs.iterdir())
        self.assertIn(FAILURE.encode(), logs[0].read_bytes())
        self.assertIn(b'Installed pinned NDK', logs[1].read_bytes())

    def test_exhaustion_keeps_three_logs_and_final_stage(self):
        result = self.run_installer(['corrupt'], expectCleared=True)
        self.assertEqual(result.returncode, 7, result.stderr)
        self.assertEqual(self.calls(), 3)
        self.assertEqual(len(list(self.logs.iterdir())), 3)
        self.assertEqual((self.sdk / '.temp/PackageOperation01/archive.zip').read_text(), 'corrupt')

    def test_unrelated_failure_not_retried(self):
        result = self.run_installer(['license', 'success'])
        self.assertEqual(result.returncode, 9)
        self.assertEqual(self.calls(), 1)

    def test_mixed_errors_cannot_borrow_retry(self):
        self.assertFalse(SDK.archive_failure_only(FAILURE + '\nError: License not accepted'))
        self.assertFalse(SDK.archive_failure_only('Gradle compile failed'))
        self.assertTrue(SDK.archive_failure_only(FAILURE))

    def test_stage_predating_invocation_not_removed(self):
        (self.sdk / '.temp/PackageOperation01').mkdir()
        result = self.run_installer(['corrupt', 'success'])
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls(), 1)
        self.assertTrue((self.sdk / '.temp/PackageOperation01/archive.zip').exists())

    def test_installed_package_never_cleaned(self):
        target = self.sdk / 'ndk/27.1.12297006'
        target.mkdir()
        (target / 'package.xml').write_text('working')
        result = self.run_installer(['corrupt', 'success'])
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls(), 1)
        self.assertEqual((target / 'package.xml').read_text(), 'working')

    def test_escaping_or_symlink_stage_refused(self):
        for symlink in (False, True):
            with self.subTest(symlink=symlink):
                target = self.sdk / 'ndk/27.1.12297006/.installer'
                target.mkdir(parents=True, exist_ok=True)
                stage = self.sdk / '.temp/PackageOperation03'
                outside = self.root / 'outside'
                outside.mkdir(exist_ok=True)
                (outside / 'valuable').write_text('keep')
                if symlink:
                    stage.symlink_to(outside, target_is_directory=True)
                else:
                    stage = outside
                (target / '.installData').write_text('path=' + str(stage) + '\nclass=com.android.repository.impl.installer.BasicInstaller\n')
                with self.assertRaises(ValueError):
                    SDK.clear_owned_stage(self.sdk, PACKAGE, set(), set())
                self.assertEqual((outside / 'valuable').read_text(), 'keep')


if __name__ == '__main__':
    unittest.main()
