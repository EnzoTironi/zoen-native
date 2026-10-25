"""Real immutable Git histories exercise the frontend-only Rust CI boundary."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
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


class FrontendScopeTests(unittest.TestCase):
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

    def test_verified_frontend_and_app_tests_skip_only_rust(self):
        self.edit_frontend()
        self.write("apps/zoen/src/core/scope-fixture.test.ts", "export const contract = 'UI';\n")
        result = self.classify()
        self.assertTrue(result["frontend_only"], result)
        self.assertFalse(result["documentation_only"], result)
        self.assertFalse(result["run_rust"], result)

    def test_mixed_native_generated_config_and_unknown_inputs_keep_rust(self):
        for path in ("Cargo.toml", "Cargo.lock", "crates/fixture/src/lib.rs", "build.rs",
                     "scripts/dev.mjs", ".github/workflows/ci.yml", "devtools/input.ts",
                     "apps/zoen/app.config.ts", "apps/zoen/package.json", "apps/zoen/tsconfig.json",
                     "apps/zoen/modules/native.swift", "apps/zoen/src/core/generated/Command.ts",
                     "apps/zoen/src/miniapps/vendor/resource.ts", "apps/zoen/src/core/types.ts",
                     "apps/zoen/src/core/driver.ts", "apps/zoen/src/media/native-media.ts",
                     "apps/zoen/src/media/driver.ios.ts", "apps/zoen/src/schema.d.ts",
                     "apps/zoen/src/contract.json", "packages/zoen-miniapp-sdk/src/shared.ts",
                     "apps/zoen/src/app/template.html", "README.md"):
            with self.subTest(path=path):
                self.git("reset", "--hard", self.base)
                self.git("clean", "-fdq")
                self.edit_frontend()
                self.write(path, "Changed input\n")
                self.assert_full(self.classify())

    def test_push_unknown_empty_and_invalid_revisions_never_skip(self):
        self.assert_full(self.classify(self.base))
        self.edit_frontend()
        head = self.commit()
        for event in ("push", "workflow_dispatch", "pull_request_target", ""):
            self.assert_full(self.classify(head, event))
        for base, target in (("0" * 40, head), (self.base, "0" * 40)):
            self.assert_full(SCOPE.classify(base, target, "pull_request"))

    def test_rename_delete_symlink_and_mode_changes_never_skip(self):
        for operation in ("rename", "delete", "symlink", "executable"):
            with self.subTest(operation=operation):
                self.git("reset", "--hard", self.base)
                self.git("clean", "-fdq")
                if operation == "rename":
                    self.git("mv", FRONTEND, "apps/zoen/src/app/renamed.tsx")
                elif operation == "delete":
                    Path(FRONTEND).unlink()
                elif operation == "symlink":
                    Path(FRONTEND).unlink()
                    Path(FRONTEND).symlink_to("../../../../outside")
                else:
                    self.edit_frontend()
                    Path(FRONTEND).chmod(0o755)
                self.assert_full(self.classify())

    def test_binary_or_empty_typescript_is_not_a_verified_frontend_input(self):
        for value in (b"\0binary", b"\xff", b""):
            self.write(FRONTEND, value)
            self.assert_full(self.classify())

    def test_direct_or_shared_json_consumers_keep_the_full_lane(self):
        for consumer, body in (
            ("crates/fixture/src/lib.rs", f'const INPUT: &str = include_str!("../../../{FRONTEND}");\n'),
            ("crates/fixture/src/lib.rs", 'fn input() { let _ = "scope-fixture.tsx"; }\n'),
            ("contracts/shared.json", json.dumps({"inputs": [FRONTEND]})),
            ("tools/read-frontend.py", f'INPUT = "{FRONTEND}"\n'),
            ("crates/fixture/src/lib.rs", 'fn dynamic_input() { let _ = "apps/zoen/src"; }\n'),
            ("crates/fixture/src/lib.rs", 'fn dynamic_input() { let _ = ["apps", "zoen", "src"]; }\n'),
        ):
            with self.subTest(consumer=consumer, body=body):
                self.git("reset", "--hard", self.base)
                self.git("clean", "-fdq")
                original = self.base
                self.write(consumer, body)
                base = self.commit()
                self.edit_frontend()
                self.assert_full(self.classify(base=base))
                self.base = original

    def test_unrelated_filename_suffix_is_not_a_dependency(self):
        self.write("contracts/unrelated.json", '{"source":"durable-scope-fixture.tsx"}')
        self.base = self.commit()
        self.edit_frontend()
        result = self.classify()
        self.assertTrue(result["frontend_only"], result)

    def test_actual_manifest_declared_transitive_frontend_input_keeps_rust(self):
        original = json.loads(Path(BUNDLE_MANIFESTS[0]).read_text())
        self.assertTrue(original["sources"], "Exercise the actual compiled source manifest")
        self.assertEqual(original["resourceUri"], "ui://zoen/deckcraft/index.html")
        original["sources"][FRONTEND] = hashlib.sha256(Path(FRONTEND).read_bytes()).hexdigest()
        self.pin_manifest(original)
        self.edit_frontend()
        result = self.classify()
        self.assert_full(result)
        self.assertIn("embedded-resource", result["reason"])

    def test_nested_provenance_input_is_also_part_of_the_closure(self):
        data = json.loads(Path(BUNDLE_MANIFESTS[0]).read_text())
        data["provenance"] = {"compiled": [{"source": FRONTEND}]}
        self.pin_manifest(data)
        self.edit_frontend()
        self.assert_full(self.classify())

    def test_word_bootstrap_declared_frontend_input_keeps_rust(self):
        path = "crates/miniapps/resources/generated/wordcraft-bootstrap.manifest.json"
        self.assertIn(path, SCOPE.SOURCE_BUNDLE_MANIFESTS)
        data = json.loads(Path(path).read_text())
        self.assertEqual(data["resourceUri"], "ui://zoen/wordcraft/index.html")
        self.assertTrue(data["sources"])
        data["sources"][FRONTEND] = hashlib.sha256(Path(FRONTEND).read_bytes()).hexdigest()
        self.pin_manifest(data, path)
        self.edit_frontend()
        result = self.classify()
        self.assert_full(result)
        self.assertIn("embedded-resource", result["reason"])

    def test_word_bootstrap_empty_source_closure_never_skips_rust(self):
        path = "crates/miniapps/resources/generated/wordcraft-bootstrap.manifest.json"
        self.pin_manifest({"sources": {}}, path)
        self.edit_frontend()
        result = self.classify()
        self.assert_full(result)
        self.assertIn("Invalid bundled source closure", result["reason"])

    def test_manifest_parse_shape_and_hash_failures_default_full(self):
        original = self.base
        for data in (b"not JSON", b"[]", b'{"sources":{}}', b'{"sources":{"x":42}}'):
            self.git("reset", "--hard", original)
            path = BUNDLE_MANIFESTS[0]
            self.write(path, data)
            SCOPE.FRONTEND_CONTRACTS[path] = hashlib.sha256(data).hexdigest()
            self.base = self.commit()
            self.edit_frontend()
            self.assert_full(self.classify())

    def test_unknown_manifest_or_incomplete_profile_stays_full(self):
        self.write("crates/miniapps/resources/new.manifest.json", '{"sources":{}}')
        self.base = self.commit()
        self.edit_frontend()
        result = self.classify()
        self.assert_full(result)
        self.assertIn("Unknown or missing", result["reason"])
        self.git("reset", "--hard", self.base)
        SCOPE.FRONTEND_CONTRACTS.pop(SCOPE.RESOURCE_MANIFESTS[0])
        self.edit_frontend()
        self.assert_full(self.classify())

    def test_contract_drift_in_either_endpoint_stays_full(self):
        for path in self.pins:
            with self.subTest(path=path):
                self.git("reset", "--hard", self.base)
                self.git("clean", "-fdq")
                self.write(path, Path(path).read_bytes() + b"\n# drift\n")
                base = self.commit()
                self.edit_frontend()
                result = self.classify(base=base)
                self.assert_full(result)
                self.assertIn("contract drift", result["reason"])
                # Restoring the head still cannot hide the base contract change.
                self.write(path, (SOURCE.parents[1] / path).read_bytes())
                self.assert_full(self.classify(base=base))

    def test_stale_side_branch_or_ambiguous_merge_bases_keep_rust(self):
        self.git("checkout", "-qb", "frontend")
        self.edit_frontend()
        head = self.commit()
        self.git("checkout", "-qb", "base", self.base)
        self.write("crates/fixture/src/lib.rs", "pub fn new_consumer() {}\n")
        base = self.commit()
        self.assert_full(self.classify(head, base=base))
        tree = self.git("rev-parse", self.base + "^{tree}")
        first = self.git("commit-tree", tree, "-p", self.base, "-m", "first")
        second = self.git("commit-tree", tree, "-p", self.base, "-m", "second")
        left = self.git("commit-tree", tree, "-p", first, "-p", second, "-m", "left")
        right = self.git("commit-tree", tree, "-p", second, "-p", first, "-m", "right")
        self.assert_full(self.classify(right, base=left))

    def test_json_and_github_outputs_only_skip_after_positive_classification(self):
        self.edit_frontend()
        head = self.commit()
        output, summary = Path(".git/output"), Path(".git/summary")
        env = {**self.env, "GITHUB_OUTPUT": str(output), "GITHUB_STEP_SUMMARY": str(summary)}
        run = subprocess.run([sys.executable, str(SOURCE), "--base", self.base, "--head", head,
                              "--event", "pull_request"], env=env, capture_output=True, text=True)
        self.assertEqual(run.returncode, 0, run.stderr)
        result = json.loads(run.stdout)
        self.assertFalse(result["run_rust"], result)
        self.assertIn("run_rust=false", output.read_text())
        self.assertIn("frontend_only=true", output.read_text())
        self.assertIn("Rust job SKIPPED: no application Rust compilation or journeys", summary.read_text())
        run = subprocess.run([sys.executable, str(SOURCE), "--base", "missing", "--head", head,
                              "--event", "pull_request"], env=env, capture_output=True, text=True)
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertTrue(json.loads(run.stdout)["run_rust"])
        self.assertTrue(output.read_text().endswith("run_rust=true\n"))

    def test_workflow_keeps_guards_client_scope_and_full_failure_fallback(self):
        workflow = (SOURCE.parents[1] / ".github/workflows/ci.yml").read_text()
        rust = re.search(r"(?ms)^  rust:\n(.*?)(?=^  [a-z]|\Z)", workflow)[1]
        shards = re.search(r"(?ms)^  rust-tests:\n(.*?)(?=^  [a-z]|\Z)", workflow)[1]
        local = re.search(r"(?ms)^  local-integrity:\n(.*?)(?=^  [a-z]|\Z)", workflow)[1]
        self.assertIn("!cancelled()", shards)
        self.assertIn("needs.change-scope.result != 'success' ||", shards)
        self.assertIn("needs.change-scope.outputs.run_rust != 'false'", shards)
        self.assertIn("always()", rust)
        self.assertIn("needs.change-scope.result != 'success' ||", rust)
        self.assertIn("needs.change-scope.outputs.run_rust != 'false'", rust)
        for guard in ("Shared Cargo source qualification guard", "Observed UI action choice guard",
                      "Expo Core shared-target entrypoint contracts"):
            self.assertIn(guard, local)
            self.assertNotIn(guard, rust)
        self.assertIn("timeout-minutes: 60", shards)
        self.assertIn("cargo test --workspace --exclude zoen-cli --no-fail-fast", shards)
        self.assertIn("python3 scripts/ci-rust-shards.py 0", shards)
        self.assertIn("python3 scripts/ci-rust-shards.py 1", shards)
        self.assertIn("python3 scripts/ci-rust-shards.py 2", shards)
        self.assertIn("cargo clippy --workspace --all-targets -- -D warnings", shards)
        self.assertIn("fail-fast: false", shards)
        self.assertIn('run: ${{ matrix.command }}', shards)
        self.assertIn('needs: [change-scope, rust-tests]', rust)
        self.assertIn('run: test "$RUST_RESULT" = success', rust)
        gate = re.search(r'^        run: (.+)$', rust, re.MULTILINE)[1]
        for result in ("success", "failure", "cancelled", "skipped", ""):
            run = subprocess.run(['sh', '-c', gate],
                                 env={**self.env, "RUST_RESULT": result})
            self.assertEqual(run.returncode == 0, result == "success")
        self.assertNotIn("frontend_only", workflow[workflow.index("  web-e2e:"):])
        self.assertIn("documentation_only != 'true'", workflow[workflow.index("  web-e2e:"):])


if __name__ == "__main__":
    unittest.main()
