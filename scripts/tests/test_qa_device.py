"""Exercise the real QA wrapper with a fake executable; never contact a device."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
WRAPPER = ROOT / "scripts/qa-device.py"
FAKE = r'''
import json, os, sys
from pathlib import Path
event = {"argv": sys.argv[1:], "cwd": os.getcwd(),
         "developerDir": os.environ.get("DEVELOPER_DIR"),
         "sentinel": os.environ.get("QA_DEVICE_TEST_SENTINEL")}
with Path(os.environ["QA_DEVICE_TEST_LOG"]).open("a") as output:
    output.write(json.dumps(event) + "\n")
print("fake-driver stdout")
print("fake-driver stderr", file=sys.stderr)
sys.exit(int(os.environ.get("QA_DEVICE_TEST_EXIT", "0")))
'''


class QaDeviceTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="zoen-qa-device-")
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name).resolve()
        self.log = self.directory / "events.jsonl"
        binary = self.directory / "bin/agent-device"
        binary.parent.mkdir()
        binary.write_text(f"#!{sys.executable}\n" + FAKE)
        binary.chmod(0o755)
        self.environment = dict(
            os.environ,
            PATH=str(binary.parent),
            DEVELOPER_DIR="/synthetic/Xcode with spaces/Contents/Developer",
            QA_DEVICE_TEST_SENTINEL="preserve this value",
            QA_DEVICE_TEST_LOG=str(self.log),
        )

    def run_wrapper(self, *arguments, extra_env=None):
        return subprocess.run(
            [sys.executable, str(WRAPPER), *arguments],
            cwd=self.directory,
            env=dict(self.environment, **(extra_env or {})),
            capture_output=True,
            text=True,
            timeout=5,
        )

    def events(self):
        return ([json.loads(line) for line in self.log.read_text().splitlines()]
                if self.log.exists() else [])

    def test_fill_never_dispatches_for_empty_nonempty_refs_or_labels(self):
        for arguments in [
            ("fill", 'id="message-input"', "", "--session", "synthetic"),
            ("fill", "@e77", "replacement"),
            ("fill", 'label="Mensagem"', "replacement", "--settle"),
            ("fill", "390", "508", "replacement"),
            ("fill",),
        ]:
            with self.subTest(arguments=arguments):
                result = self.run_wrapper(*arguments)
                self.assertEqual(2, result.returncode)
                self.assertIn("refused before dispatch", result.stderr)
                self.assertIn("leave the new draft intact", result.stderr)
                self.assertIn("Do not fall back", result.stderr)
                self.assertEqual("", result.stdout)
                self.assertEqual([], self.events())

    def test_type_never_dispatches_including_empty_and_literal_newline(self):
        for text in ("", "new text", "\n"):
            with self.subTest(text=text):
                result = self.run_wrapper("type", text, "--session", "synthetic")
                self.assertEqual(2, result.returncode)
                self.assertIn("fill and type are disabled", result.stderr)
                self.assertEqual([], self.events())

    def test_destructive_deployment_never_dispatches_even_for_a_private_candidate(self):
        for verb in ("reinstall", "uninstall"):
            for private in ({}, {"ZOEN_QA_DRIVER_ROOT": "/invalid/private/candidate"}):
                with self.subTest(verb=verb, private=bool(private)):
                    result = self.run_wrapper(
                        verb, "xyz.tironi.zoen", "/synthetic/Zoen.app",
                        "--session", "synthetic", extra_env=private,
                    )
                    self.assertEqual(2, result.returncode)
                    self.assertIn("refused before dispatch", result.stderr)
                    self.assertIn("erase the app's local data", result.stderr)
                    self.assertIn("use install <app> <binary>", result.stderr)
                    self.assertEqual([], self.events())

    def test_update_install_passes_through_once_without_reinstalling(self):
        arguments = ("install", "xyz.tironi.zoen", "/synthetic/Zoen.app",
                     "--session", "synthetic")
        result = self.run_wrapper(*arguments)
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual([list(arguments)], [event["argv"] for event in self.events()])

    def test_missing_verb_leading_options_and_malformed_verbs_fail_closed(self):
        for arguments in [(), ("--session", "synthetic", "fill", "@e1", ""),
                          ("--", "fill", "@e1", ""), ("-h",),
                          ("--unknown", "snapshot"), (" fill", "@e1", ""),
                          ("Fill", "@e1", ""), ("snapshot;fill", "@e1", "")]:
            with self.subTest(arguments=arguments):
                result = self.run_wrapper(*arguments)
                self.assertEqual(2, result.returncode)
                self.assertIn("command verb first", result.stderr)
                self.assertEqual([], self.events())

    def test_read_command_preserves_literal_argv_environment_cwd_and_streams(self):
        arguments = ("get", "text", 'id="field with spaces"', "--session", "synthetic",
                     "$(touch should-not-exist)", "`touch also-not-created`", "; literal")
        result = self.run_wrapper(*arguments)
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual("fake-driver stdout\n", result.stdout)
        self.assertEqual("fake-driver stderr\n", result.stderr)
        self.assertEqual([{
            "argv": list(arguments), "cwd": str(self.directory),
            "developerDir": self.environment["DEVELOPER_DIR"],
            "sentinel": self.environment["QA_DEVICE_TEST_SENTINEL"],
        }], self.events())
        self.assertFalse((self.directory / "should-not-exist").exists())
        self.assertFalse((self.directory / "also-not-created").exists())

    def test_snapshot_and_navigation_pass_through_once_without_retry(self):
        for arguments in [("snapshot", "-i", "--session", "synthetic"),
                          ("press", 'id="navigation-back"', "--session", "synthetic")]:
            with self.subTest(arguments=arguments):
                before = len(self.events())
                result = self.run_wrapper(*arguments, extra_env={"QA_DEVICE_TEST_EXIT": "37"})
                self.assertEqual(37, result.returncode)
                self.assertEqual(before + 1, len(self.events()))
                self.assertEqual(list(arguments), self.events()[-1]["argv"])

    def test_absent_driver_does_not_fall_back_to_another_binary(self):
        empty_path = self.directory / "empty-path"
        empty_path.mkdir()
        result = self.run_wrapper("snapshot", extra_env={"PATH": str(empty_path)})
        self.assertEqual(127, result.returncode)
        self.assertIn("no command was dispatched", result.stderr)
        self.assertEqual([], self.events())


if __name__ == "__main__":
    unittest.main()
