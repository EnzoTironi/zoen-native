"""Exercise qualified/default dispatch with real temporary Git and a fake driver."""

import importlib.util
import io
from contextlib import redirect_stderr
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import qa_device_candidate as candidate

spec = importlib.util.spec_from_file_location("qa_device_wrapper_test", SCRIPTS / "qa-device.py")
wrapper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wrapper)


class QualifiedDriverTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="zoen-qualified-qa-")
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name).resolve()
        self.root = self.directory / "candidate"
        self.root.mkdir()
        self.entry = self.root / "bin/agent-device.mjs"
        self.entry.parent.mkdir()
        self.log = self.directory / "dispatch.jsonl"
        self.entry.write_text(
            'import json,os,sys\nfrom pathlib import Path\n'
            'with Path(os.environ["QA_TEST_LOG"]).open("a") as out:\n'
            ' out.write(json.dumps({"argv":sys.argv[1:],'
            '"state":os.environ["AGENT_DEVICE_STATE_DIR"]})+"\\n")\n'
            'sys.exit(int(os.environ.get("QA_TEST_EXIT","0")))\n'
        )
        self.environment = dict(
            os.environ, ZOEN_QA_DRIVER_ROOT=str(self.root),
            ZOEN_QA_NODE_BIN=sys.executable,
            AGENT_DEVICE_HOME=str(self.directory / "config"),
            AGENT_DEVICE_STATE_DIR=str(self.directory / "daemon"),
            AGENT_DEVICE_IOS_RUNNER_DERIVED_PATH=str(self.directory / "derived"),
            AGENT_DEVICE_IOS_BUNDLE_ID="dev.agentdevice.zoenqa.fixture",
            AGENT_DEVICE_IOS_RUNNER_TEST_BUNDLE_ID="dev.agentdevice.zoenqa.fixture.uitests",
            QA_TEST_LOG=str(self.log),
            GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1",
        )
        for key in ["AGENT_DEVICE_CONFIG", "AGENT_DEVICE_DAEMON_BASE_URL", "AGENT_DEVICE_DAEMON_AUTH_TOKEN"]:
            self.environment.pop(key, None)
        git = shutil.which("git")
        if not git:
            self.fail("Git is required to exercise the real source-verification boundary")
        self.git = git
        self.git_run("init", "-q")
        self.git_run("add", "bin/agent-device.mjs")
        self.git_run("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                     "commit", "-qm", "synthetic qualified package")
        self.commit = self.git_run("rev-parse", "HEAD").stdout.strip()

    def git_run(self, *args):
        return subprocess.run([self.git, "-C", str(self.root), *args],
                              env=self.environment, capture_output=True, text=True, check=True)

    def run_wrapper(self, *args, extra=None, commit=None):
        errors = io.StringIO()
        with patch.dict(os.environ, dict(self.environment, **(extra or {})), clear=True), \
             patch.object(candidate, "QUALIFIED_COMMIT", self.commit if commit is None else commit), \
             redirect_stderr(errors):
            result = wrapper.main(list(args))
        self.stderr = errors.getvalue()
        return result

    def events(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def test_exact_clean_candidate_receives_literal_fill_type_and_read_once(self):
        for args in [("fill", 'id="editor"', ""),
                     ("fill", 'id="editor"', "👨‍👩‍👧‍👦\n$(touch not-created)"),
                     ("type", " suffix"), ("snapshot", "-i")]:
            with self.subTest(args=args):
                before = len(self.events())
                self.assertEqual(0, self.run_wrapper(*args))
                self.assertEqual(before + 1, len(self.events()))
                self.assertEqual(list(args), self.events()[-1]["argv"])
                self.assertEqual(self.environment["AGENT_DEVICE_STATE_DIR"], self.events()[-1]["state"])
        self.assertFalse((self.directory / "not-created").exists())

    def test_wrong_commit_refuses_before_any_driver_dispatch(self):
        self.assertEqual(2, self.run_wrapper("fill", "@e1", "", commit="0" * 40))
        self.assertEqual([], self.events())

    def test_modified_staged_and_untracked_packages_cannot_dispatch_or_fallback(self):
        original = self.entry.read_text()
        self.entry.write_text(original + "# modified\n")
        self.assertEqual(2, self.run_wrapper("fill", "@e1", "x"))
        self.git_run("add", "bin/agent-device.mjs")
        self.assertEqual(2, self.run_wrapper("type", "x"))
        self.git_run("reset", "--hard", "-q", "HEAD")
        (self.root / "unexpected.js").write_text("unreviewed")
        self.assertEqual(2, self.run_wrapper("snapshot"))
        self.assertEqual([], self.events())

    def test_missing_relative_or_shared_private_paths_are_refused(self):
        for extra in [{"AGENT_DEVICE_STATE_DIR": ""},
                      {"AGENT_DEVICE_HOME": "relative"},
                      {"AGENT_DEVICE_STATE_DIR": self.environment["AGENT_DEVICE_HOME"]},
                      {"AGENT_DEVICE_IOS_RUNNER_TEST_BUNDLE_ID": "global.runner.uitests"},
                      {"ZOEN_QA_DRIVER_ROOT": str(self.entry.parent)}]:
            with self.subTest(extra=extra):
                self.assertEqual(2, self.run_wrapper("type", "x", extra=extra))
                self.assertEqual([], self.events())

    def test_other_connection_configs_are_refused_without_logging_values(self):
        for key in ["AGENT_DEVICE_CONFIG", "AGENT_DEVICE_DAEMON_BASE_URL", "AGENT_DEVICE_DAEMON_AUTH_TOKEN"]:
            self.assertEqual(2, self.run_wrapper("fill", "@e1", "", extra={key: "private-value"}))
            self.assertNotIn("private-value", self.stderr)
        self.assertEqual(2, self.run_wrapper("fill", "@e1", "", "--config", "other.json"))
        self.assertEqual([], self.events())

    def test_candidate_exit_is_preserved_without_retry(self):
        self.assertEqual(37, self.run_wrapper("type", "x", extra={"QA_TEST_EXIT": "37"}))
        self.assertEqual(1, len(self.events()))

    def test_unknown_existing_daemon_profile_is_not_adopted(self):
        state = Path(self.environment["AGENT_DEVICE_STATE_DIR"])
        state.mkdir()
        (state / "existing-daemon.json").write_text("synthetic unknown daemon")
        self.assertEqual(2, self.run_wrapper("type", "x"))
        self.assertIn("fresh empty daemon directory", self.stderr)
        self.assertEqual([], self.events())

    def test_foreign_or_damaged_profile_marker_does_not_dispatch(self):
        state = Path(self.environment["AGENT_DEVICE_STATE_DIR"])
        state.mkdir()
        marker = state / ".zoen-qualified-source.json"
        for text in ["broken JSON", json.dumps({"source": str(self.root), "commit": "0" * 40})]:
            marker.write_text(text)
            self.assertEqual(2, self.run_wrapper("fill", "@e1", "x"))
            self.assertEqual([], self.events())


if __name__ == "__main__":
    unittest.main()
