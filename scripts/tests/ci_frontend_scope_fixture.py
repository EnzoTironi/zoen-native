"""Shared immutable Git fixture for frontend CI boundary tests."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parents[1] / "ci-documentation-scope.py"
sys.path.insert(0, str(SOURCE.parent))
from ci_frontend_inputs import BUNDLE_MANIFESTS
SPEC = importlib.util.spec_from_file_location("frontend_scope", SOURCE)
SCOPE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SCOPE)
FRONTEND = "apps/zoen/src/app/scope-fixture.tsx"


class FrontendRepositoryFixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.previous = Path.cwd()
        os.chdir(self.temp.name)
        self.addCleanup(os.chdir, self.previous)
        self.env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
        self.env.update(GIT_AUTHOR_NAME="Scope fixture", GIT_COMMITTER_NAME="Scope fixture",
                        GIT_AUTHOR_EMAIL="scope@example.invalid", GIT_COMMITTER_EMAIL="scope@example.invalid",
                        GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1")
        self.git("init", "-q")
        for key, value in (("commit.gpgsign", "false"), ("maintenance.auto", "false"), ("gc.auto", "0")):
            self.git("config", key, value)
        self.pins = dict(SCOPE.FRONTEND_CONTRACTS)
        self.addCleanup(setattr, SCOPE, "FRONTEND_CONTRACTS", self.pins)
        SCOPE.FRONTEND_CONTRACTS = dict(self.pins)
        for path in self.pins:
            self.write(path, (SOURCE.parents[1] / path).read_bytes())
        self.write(FRONTEND, "export default function Screen() { return null; }\n")
        self.write("crates/fixture/src/lib.rs", "pub fn fixture() {}\n")
        self.base = self.commit()

    def git(self, *args):
        return subprocess.check_output(["git", *args], env=self.env, stderr=subprocess.PIPE).decode().strip()

    def write(self, path, body):
        file = Path(path)
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_bytes(body.encode() if isinstance(body, str) else body)

    def commit(self):
        self.git("add", ".")
        self.git("commit", "-qm", "scope fixture")
        return self.git("rev-parse", "HEAD")

    def edit_frontend(self):
        self.write(FRONTEND, "export default function Screen() { return 'changed'; }\n")

    def classify(self, head=None, event="pull_request", base=None):
        return SCOPE.classify(base or self.base, head or self.commit(), event)

    def assert_full(self, result):
        self.assertTrue(result["run_rust"], result)
        self.assertFalse(result["frontend_only"], result)

    def pin_manifest(self, data, path=None):
        path = path or BUNDLE_MANIFESTS[0]
        self.write(path, json.dumps(data))
        SCOPE.FRONTEND_CONTRACTS[path] = hashlib.sha256(Path(path).read_bytes()).hexdigest()
        self.base = self.commit()

