"""Platform UI and mixed static metadata over real immutable Git histories."""
import importlib.util
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

from ci_frontend_scope_fixture import FrontendRepositoryFixture, SOURCE, SCOPE

ROOT = SOURCE.parents[1]
SPEC = importlib.util.spec_from_file_location("ui_expo_scope", ROOT / "scripts/expo-artifact-scope.py")
EXPO = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(EXPO)
UI = "apps/zoen/src/components/message-actions.ios.tsx"
DOCUMENT = "docs/roadmap/telegram/messaging.json"


class UiScopeTests(FrontendRepositoryFixture):
    def expo(self, head):
        tree = self.git("merge-tree", "--write-tree", self.base, head)
        candidate = self.git("commit-tree", tree, "-p", self.base, "-p", head, "-m", "canonical merge")
        self.git("checkout", "--detach", "-q", candidate)
        try:
            with patch.object(EXPO, "graph", side_effect=AssertionError("No changed Cargo inputs")):
                return EXPO.classify(self.base, head, "pull_request", "/must-not-run-cargo", candidate)
        finally:
            self.git("checkout", "--detach", "-q", head)

    def assert_full_batch(self, head):
        self.assert_full(self.classify(head))
        artifacts = self.expo(head)
        self.assertTrue(all(artifacts["run_" + job] for job in EXPO.JOBS), artifacts)

    def ui(self, path=UI):
        self.write(path, "export function Actions() { return null; }\n")

    def documents(self):
        self.write(DOCUMENT, json.dumps([{"id": "message-actions", "gap": "Awaiting Enzo"}]))

    def assert_ui_lane(self, head, documents=False):
        result = self.classify(head)
        self.assertTrue(result["frontend_only"], result)
        self.assertFalse(result["run_rust"], result)
        self.assertEqual(result["has_documentation"], documents, result)
        self.assertEqual(result["documentation_paths"], [DOCUMENT] if documents else [])
        self.assertFalse(result["documentation_only"], result)
        artifacts = self.expo(head)
        self.assertFalse(artifacts["run_android"], artifacts)
        self.assertTrue(artifacts["run_android_js"], artifacts)
        self.assertTrue(artifacts["run_client"], artifacts)
        self.assertTrue(artifacts["run_release_web"], artifacts)

    def test_ios_and_android_react_components_keep_client_and_bundle_checks(self):
        for path in [UI, "apps/zoen/src/components/message-actions.android.tsx"]:
            with self.subTest(path=path):
                self.git("reset", "--hard", self.base)
                self.git("clean", "-fdq")
                self.ui(path)
                self.assert_ui_lane(self.commit())

    def test_mixed_platform_ui_and_static_metadata_uses_both_validators(self):
        self.ui()
        self.documents()
        head = self.commit()
        self.assert_ui_lane(head, documents=True)
        output = subprocess.check_output([
            sys.executable, str(SOURCE), "--base", self.base, "--head", head,
            "--event", "pull_request", "--check-documents"], env=self.env)
        result = json.loads(output)
        self.assertEqual(result["documentation_paths"], [DOCUMENT])
        self.assertIn("validation", result)

    def test_unqualified_native_paths_and_unknown_metadata_keep_every_artifact(self):
        for path in ["apps/zoen/src/media/driver.ios.ts", "apps/zoen/src/components/native-bridge.ios.tsx",
                     "apps/zoen/src/components/bridge.native.tsx", "apps/zoen/src/app/screen.ios.tsx",
                     "apps/zoen/modules/zoen-core/ios/Bridge.swift", "apps/zoen/modules/zoen-core/android/Bridge.kt",
                     "docs/roadmap/telegram/unknown.json", "scripts/ci_ui_scope.py"]:
            with self.subTest(path=path):
                self.git("reset", "--hard", self.base)
                self.git("clean", "-fdq")
                self.ui()
                self.documents()
                self.write(path, "unknown input")
                head = self.commit()
                self.assert_full_batch(head)

    def test_changed_acceptance_flags_or_invalid_json_do_not_become_prose(self):
        self.write("apps/zoen/acceptance.json", '{"currentAcceptanceSummary":"before","accepted":false}')
        self.base = self.commit()
        for document, body in [("apps/zoen/acceptance.json", '{"currentAcceptanceSummary":"after","accepted":true}'),
                               (DOCUMENT, '{"duplicate":1,"duplicate":2}'), (DOCUMENT, '{"number":NaN}')]:
            with self.subTest(document=document, body=body):
                self.git("reset", "--hard", self.base)
                self.git("clean", "-fdq")
                self.ui()
                self.write(document, body)
                head = self.commit()
                self.assert_full_batch(head)

    def test_platform_ui_still_honors_real_foreign_consumers(self):
        self.write("crates/fixture/src/lib.rs", 'const UI: &str = include_str!("../../../' + UI + '");\n')
        self.base = self.commit()
        self.ui()
        head = self.commit()
        self.assert_full_batch(head)

    def test_static_metadata_consumed_by_runtime_stays_a_runtime_input(self):
        self.write("crates/fixture/src/lib.rs", 'const DATA: &str = include_str!("../../../' + DOCUMENT + '");\n')
        self.base = self.commit()
        self.ui()
        self.documents()
        head = self.commit()
        self.assert_full_batch(head)

    def test_mixed_document_integrity_does_not_parse_ui_as_markdown(self):
        self.ui()
        self.documents()
        document = json.loads(Path(DOCUMENT).read_text())
        document[0]["gap"] = "[Source](missing.md)"
        self.write(DOCUMENT, json.dumps(document))
        head = self.commit()
        result = self.classify(head)
        self.assertTrue(result["has_documentation"], result)
        with self.assertRaises(subprocess.CalledProcessError):
            SCOPE.validate_documents(head, result["documentation_paths"])

    def test_combined_batches_keep_the_existing_128_path_bound(self):
        for index in range(128):
            self.ui("apps/zoen/src/components/action" + str(index) + ".tsx")
        self.documents()
        self.assert_full_batch(self.commit())

    def test_platform_ui_aliases_and_executable_modes_do_not_qualify(self):
        for mode in ("symlink", "executable"):
            with self.subTest(mode=mode):
                self.git("reset", "--hard", self.base)
                self.git("clean", "-fdq")
                self.ui()
                if mode == "symlink":
                    Path(UI).unlink()
                    Path(UI).symlink_to("../../../../outside")
                else:
                    Path(UI).chmod(0o755)
                self.assert_full_batch(self.commit())


class WorkflowRoutingTests(unittest.TestCase):
    def test_mixed_metadata_retains_the_document_integrity_job(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertIn('has_documentation: ${{ steps.scope.outputs.has_documentation }}', workflow)
        job = workflow.split('\n  documentation-integrity:\n', 1)[1].split('\n  local-integrity:', 1)[0]
        condition = next(line for line in job.splitlines() if line.strip().startswith('if:'))
        self.assertIn(' || ', condition)
        self.assertIn("outputs.documentation_only == 'true'", job)
        self.assertIn("outputs.has_documentation == 'true'", job)
        self.assertIn('--check-documents', job)

    def test_new_selector_inputs_trigger_expo_and_match_the_reviewed_workflow_contracts(self):
        workflow = (ROOT / '.github/workflows/expo.yml').read_text()
        for path in ('scripts/ci_ui_scope.py', 'scripts/tests/test_ci_ui_scope.py'):
            self.assertEqual(workflow.count("'" + path + "'"), 2, path)
        for path in ('.github/workflows/ci.yml', '.github/workflows/expo.yml'):
            self.assertEqual(hashlib.sha256((ROOT / path).read_bytes()).hexdigest(),
                             SCOPE.FRONTEND_CONTRACTS[path])
