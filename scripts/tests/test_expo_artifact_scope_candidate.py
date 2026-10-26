"""Actual Git merge DAGs and the required Android check, without compilation."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import textwrap
import unittest

from test_expo_artifact_scope import Repository, ROOT, SCOPE, CARGO

UI = "apps/zoen/src/app/ui-fixture.tsx"


class CandidateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.repo = Repository(Path(self.temp.name) / "repo")
        self.cwd = Path.cwd()
        os.chdir(self.repo.root)
        self.addCleanup(os.chdir, self.cwd)
        for path in SCOPE.FRONTEND_PROOF.FRONTEND_CONTRACTS:
            self.repo.write(path, (ROOT / path).read_text())
        self.repo.write(UI, "export const screen = 'original';\n")
        self.base = self.repo.commit("frontend baseline")

    def commit_ui(self):
        self.repo.write(UI, "export const screen = 'changed';\n")
        return self.repo.commit("UI change")

    def candidate(self, base, head, tree=None, parents=None):
        tree = tree or self.repo.git("merge-tree", "--write-tree", base, head)
        args = ["commit-tree", tree]
        for parent in parents or [base, head]:
            args += ["-p", parent]
        value = self.repo.git(*args, "-m", "candidate")
        self.repo.git("checkout", "--detach", "-q", value)
        return value

    def classify(self, base, head, candidate, event="pull_request"):
        return SCOPE.classify(base, head, event, CARGO, candidate)

    def full(self, result):
        self.assertTrue(all(result["run_" + name] for name in SCOPE.JOBS), result)
        self.assertFalse(result["run_android_js"], result)

    def test_stale_pr_head_uses_exact_clean_merge_and_retains_android_js(self):
        head = self.commit_ui()
        self.repo.git("checkout", "--detach", "-q", self.base)
        self.repo.write("services/relay/src/lib.rs", "pub fn newer_base() {}\n")
        base = self.repo.commit("base advanced")
        candidate = self.candidate(base, head)
        result = self.classify(base, head, candidate)
        self.assertFalse(result["run_android"], result)
        self.assertTrue(result["run_android_js"], result)
        self.assertTrue(result["run_client"] and result["run_release_web"], result)
        self.assertEqual(result["paths"], [UI])
        self.assertEqual(result["tested_candidate"], candidate)
        self.assertFalse((self.repo.root / "target").exists())

    def test_missing_refs_candidate_checkout_or_unknown_event_are_full(self):
        head = self.commit_ui()
        candidate = self.candidate(self.base, head)
        for base, target, merge, event in [
            (self.base, head, None, "pull_request"),
            (self.base, head, "HEAD", "pull_request"),
            ("0" * 40, head, candidate, "pull_request"),
            (self.base, "0" * 40, candidate, "pull_request"),
            (self.base, head, "0" * 40, "pull_request"),
            (self.base, head, candidate, "pull_request_target"),
            (self.base, head, candidate, "workflow_dispatch"),
        ]:
            with self.subTest(event=event, merge=merge):
                self.full(self.classify(base, target, merge, event))
        self.repo.git("checkout", "--detach", "-q", head)
        result = self.classify(self.base, head, candidate)
        self.full(result)
        self.assertIn("differs from the tested checkout", result["reason"])

    def test_forged_tree_reversed_extra_or_missing_parents_never_skip(self):
        head = self.commit_ui()
        tree = self.repo.git("rev-parse", self.base + "^{tree}")
        extra = self.repo.git("commit-tree", tree, "-p", self.base, "-m", "extra parent")
        for parents in [[head, self.base], [head], [self.base, head, extra]]:
            candidate = self.candidate(self.base, head, parents=parents)
            self.full(self.classify(self.base, head, candidate))
        # Same two parents, but an unreviewed hand-written result tree.
        tree = self.repo.git("rev-parse", self.base + "^{tree}")
        candidate = self.candidate(self.base, head, tree=tree)
        result = self.classify(self.base, head, candidate)
        self.full(result)
        self.assertIn("differs from the canonical event merge", result["reason"])

    def test_ambiguous_original_merge_bases_remain_full(self):
        tree = self.repo.git("rev-parse", self.base + "^{tree}")
        a = self.repo.git("commit-tree", tree, "-p", self.base, "-m", "a")
        b = self.repo.git("commit-tree", tree, "-p", self.base, "-m", "b")
        a2 = self.repo.git("commit-tree", tree, "-p", a, "-p", b, "-m", "a2")
        b2 = self.repo.git("commit-tree", tree, "-p", b, "-p", a, "-m", "b2")
        candidate = self.candidate(a2, b2, tree=tree)
        result = self.classify(a2, b2, candidate)
        self.full(result)
        self.assertIn("Ambiguous PR merge bases", result["reason"])

    def test_conflicted_merge_is_not_a_verified_candidate(self):
        head = self.commit_ui()
        self.repo.git("checkout", "--detach", "-q", self.base)
        self.repo.write(UI, "export const screen = 'other';\n")
        base = self.repo.commit("conflicting base")
        tree = self.repo.git("rev-parse", head + "^{tree}")
        candidate = self.candidate(base, head, tree=tree)
        self.full(self.classify(base, head, candidate))

    def test_mixed_rust_native_sdk_dependency_and_unknown_inputs_stay_full(self):
        for path in ["engines/phone/src/lib.rs", "apps/zoen/modules/native.swift",
                     "apps/zoen/patches/sdk.patch", "apps/zoen/package-lock.json",
                     "packages/zoen-miniapp-sdk/src/source.ts", "notes.txt"]:
            with self.subTest(path=path):
                self.repo.git("checkout", "--detach", "-q", self.base)
                self.commit_ui()
                self.repo.write(path, "changed input\n")
                head = self.repo.commit("mixed inputs")
                self.full(self.classify(self.base, head, self.candidate(self.base, head)))

    def test_admitted_resource_and_foreign_reader_cannot_hide_behind_ui_path(self):
        manifest = SCOPE.FRONTEND_PROOF.SOURCE_BUNDLE_MANIFESTS[0]
        original_pins = SCOPE.FRONTEND_PROOF.FRONTEND_CONTRACTS
        self.addCleanup(setattr, SCOPE.FRONTEND_PROOF, "FRONTEND_CONTRACTS", original_pins)
        for mode in ["resource", "reader"]:
            with self.subTest(mode=mode):
                self.repo.git("checkout", "--detach", "-q", self.base)
                pins = dict(original_pins)
                if mode == "resource":
                    data = json.loads(Path(manifest).read_text())
                    data["sources"][UI] = hashlib.sha256(Path(UI).read_bytes()).hexdigest()
                    self.repo.write(manifest, json.dumps(data))
                    pins[manifest] = hashlib.sha256(Path(manifest).read_bytes()).hexdigest()
                else:
                    self.repo.write("tools/consumer.py", f'input = "{UI}"\n')
                SCOPE.FRONTEND_PROOF.FRONTEND_CONTRACTS = pins
                base = self.repo.commit("input boundary")
                head = self.commit_ui()
                self.full(self.classify(base, head, self.candidate(base, head)))

    def test_push_ui_stays_full_and_cannot_substitute_another_checkout(self):
        head = self.commit_ui()
        self.full(self.classify(self.base, head, head, "push"))
        candidate = self.candidate(self.base, head)
        self.full(self.classify(self.base, head, candidate, "push"))

    def test_required_android_job_preserves_both_coverage_paths_and_fail_closed(self):
        workflow = (ROOT / ".github/workflows/expo.yml").read_text()
        job = workflow.split("\n  android-build:", 1)[1].split("\n  miniapp-resources:", 1)[0]
        self.assertIn("needs.artifact-scope.result != 'success'", job)
        self.assertIn("needs.artifact-scope.outputs.run_android != 'false'", job)
        self.assertIn("needs.artifact-scope.outputs.run_android_js == 'true'", job)
        self.assertIn("ZOEN_ANDROID_FULL:", job)
        self.assertIn("ref: ${{ github.sha }}", job)
        bundle = job.split("- name: Bundle and verify Android JavaScript", 1)[1].split("- name:", 1)[0]
        self.assertIn("if: env.ZOEN_ANDROID_FULL != 'true'", bundle)
        self.assertIn("expo export --platform android", bundle)
        self.assertIn("metadata.fileMetadata?.android?.bundle", bundle)
        self.assertIn("EXPO_OFFLINE: '1'", bundle)
        self.assertNotIn("continue-on-error", job)
        for name in ["Expo Android SDK, NDK and Rust toolchain",
                     "Android shared Cargo source qualification fixtures",
                     "Build shared Rust core and generate Expo Android project",
                     "Compile Expo release and native lifecycle regressions"]:
            step = job.split("- name: " + name, 1)[1].split("- name:", 1)[0]
            self.assertIn("if: env.ZOEN_ANDROID_FULL == 'true'", step)
        self.assertIn("SCOPE_CANDIDATE: ${{ github.sha }}", workflow)
        self.assertIn('--candidate "$SCOPE_CANDIDATE"', workflow)
        triggers = workflow.split("\npermissions:", 1)[0]
        events = [triggers.split("  pull_request:\n", 1)[1].split("  push:\n", 1)[0],
                  triggers.split("  push:\n", 1)[1]]
        for event in events:
            for path in ["scripts/expo_artifact_candidate.py", "scripts/ci-documentation-scope.py",
                         "scripts/ci_frontend_inputs.py", "scripts/tests/test_expo_artifact_scope*.py"]:
                self.assertIn("'" + path + "'", event)

    def test_actual_workflow_export_check_requires_a_nonempty_android_bundle(self):
        workflow = (ROOT / ".github/workflows/expo.yml").read_text()
        marker = '''node - "$RUNNER_TEMP/zoen-android-js" <<'JS'\n'''
        script = textwrap.dedent(workflow.split(marker, 1)[1].split("          JS\n", 1)[0])
        node = shutil.which("node")
        self.assertIsNotNone(node, "The existing workflow installs pinned Node before this check")
        for index, (platform, content, expected) in enumerate([
            ("android", b"compiled bundle fixture", 0),
            ("web", b"compiled bundle fixture", 1),
            ("android", b"", 1),
        ]):
            with self.subTest(platform=platform, empty=not content):
                export = Path(self.temp.name) / f"export-{index}"
                export.mkdir()
                (export / "bundle.hbc").write_bytes(content)
                (export / "metadata.json").write_text(json.dumps({
                    "fileMetadata": {platform: {"bundle": "bundle.hbc"}},
                }))
                result = subprocess.run([node, "-", str(export)], input=script,
                                        capture_output=True, text=True)
                self.assertEqual(result.returncode, expected, result.stderr)


if __name__ == "__main__":
    unittest.main()
