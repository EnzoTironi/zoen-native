"""Real Git histories qualify the documentation-only CI boundary."""

import importlib.util
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest


SOURCE = Path(__file__).resolve().parents[1] / "ci-documentation-scope.py"
SPEC = importlib.util.spec_from_file_location("ci_scope", SOURCE)
SCOPE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SCOPE)
DOC = "docs/agent-runtime-capabilities.md"


class DocumentationScopeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.previous = Path.cwd()
        os.chdir(self.temp.name)
        self.addCleanup(os.chdir, self.previous)
        self.env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
        self.env.update(GIT_AUTHOR_NAME="Scope test", GIT_COMMITTER_NAME="Scope test",
                        GIT_AUTHOR_EMAIL="scope@example.invalid", GIT_COMMITTER_EMAIL="scope@example.invalid",
                        GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1")
        self.git("init", "-q")
        self.git("config", "commit.gpgsign", "false")
        self.write(DOC, "# Runtime\n\nInitial prose.\n")
        self.write("crates/documents/src/lib.rs", "pub fn current() {}\n")
        self.base = self.commit()

    def git(self, *args):
        return subprocess.check_output(["git", *args], env=self.env, stderr=subprocess.PIPE).decode().strip()

    def write(self, path, text):
        destination = Path(path)
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text)

    def commit(self):
        self.git("add", ".")
        self.git("commit", "-q", "-m", "fixture")
        return self.git("rev-parse", "HEAD")

    def classify(self, head=None, event="pull_request"):
        return SCOPE.classify(self.base, head or self.commit(), event)

    def test_only_allowlisted_prose_passes_and_validates_links(self):
        self.write(DOC, "# Runtime\n\n[Source](../crates/documents/src/lib.rs).\n")
        head = self.commit()
        result = self.classify(head)
        self.assertTrue(result["documentation_only"], result)
        SCOPE.validate_documents(head, result["paths"])

    def test_non_allowlisted_markdown_code_config_locks_resources_require_full_suite(self):
        for path in ["docs/telas-referencia.md", "docs/ux-motion.md", "docs/unknown.md",
                     "crates/documents/src/lib.rs", "Cargo.lock", "Cargo.toml", "build.rs",
                     ".github/workflows/ci.yml", "assets/data.json"]:
            with self.subTest(path=path):
                self.git("reset", "--hard", self.base)
                self.git("clean", "-fdq")
                self.write(DOC, "Changed prose\n")
                self.write(path, "Changed input\n")
                self.assertFalse(self.classify()["documentation_only"])

    def test_new_or_existing_consumer_disqualifies_even_an_allowlisted_document(self):
        self.write("crates/documents/src/lib.rs", 'const DATA: &str = include_str!("../../../' + DOC + '");\n')
        self.base = self.commit()
        self.write(DOC, "New prose\n")
        self.assertFalse(self.classify()["documentation_only"])
        # A dynamically constructed prefix can still use the document basename.
        self.write("crates/documents/src/lib.rs", 'const DATA: &[u8] = include_bytes!(concat!("../../../docs/", "agent-runtime-capabilities.md"));\n')
        self.assertFalse(self.classify()["documentation_only"])

    def test_empty_diff_push_missing_revision_and_binary_mode_never_skip(self):
        self.assertFalse(self.classify(self.base)["documentation_only"])
        self.write(DOC, "New prose\n")
        head = self.commit()
        self.assertFalse(self.classify(head, "push")["documentation_only"])
        self.assertFalse(SCOPE.classify("0" * 40, head, "pull_request")["documentation_only"])
        Path(DOC).chmod(0o755)
        self.assertFalse(self.classify()["documentation_only"])

    def test_removing_executable_mode_does_not_disguise_a_metadata_change_as_prose(self):
        Path(DOC).chmod(0o755)
        self.base = self.commit()
        Path(DOC).chmod(0o644)
        self.write(DOC, "Prose plus executable-bit removal\n")
        self.assertFalse(self.classify()["documentation_only"])

    def test_workflow_preserves_full_jobs_after_scope_failure_and_does_not_run_after_cancel(self):
        workflow = (SOURCE.parents[1] / ".github/workflows/ci.yml").read_text()
        for job in ("rust", "web-e2e"):
            body = re.search(r"(?ms)^  " + job + r":\n(.*?)(?=^  [a-z]|\Z)", workflow)[1]
            condition = next(line.strip() for line in body.splitlines() if line.startswith("    if:"))
            # A status function overrides GitHub's implicit success() predicate.
            # Empty outputs after a failed selector must keep the full job selected.
            self.assertIn("!cancelled()", condition)
            self.assertIn("needs.change-scope.outputs.documentation_only != 'true'", condition)

    def test_delete_rename_and_symlink_never_skip(self):
        for operation in ("delete", "rename", "symlink"):
            with self.subTest(operation=operation):
                self.git("reset", "--hard", self.base)
                self.git("clean", "-fdq")
                Path(DOC).unlink()
                if operation == "rename":
                    self.write("docs/agent-runtime-capabilities-execution.md", "Initial prose\n")
                if operation == "symlink":
                    Path(DOC).symlink_to("../outside")
                self.assertFalse(self.classify()["documentation_only"])

    def test_merge_uses_merge_base_and_does_not_hide_side_branch_code(self):
        self.git("checkout", "-qb", "docs")
        self.write(DOC, "New prose\n")
        self.commit()
        self.git("checkout", "-qb", "base", self.base)
        self.write("crates/documents/src/lib.rs", "pub fn base_update() {}\n")
        self.base = self.commit()
        self.git("checkout", "docs")
        self.git("merge", "--no-edit", "base")
        self.assertTrue(self.classify(self.git("rev-parse", "HEAD"))["documentation_only"])
        self.git("checkout", "-qb", "other")
        self.write("crates/documents/src/other.rs", "pub fn side() {}\n")
        self.commit()
        self.git("checkout", "docs")
        self.git("merge", "--no-ff", "--no-edit", "other")
        self.assertFalse(self.classify(self.git("rev-parse", "HEAD"))["documentation_only"])

    def test_crisscross_merge_bases_require_full_suite(self):
        tree = self.git("rev-parse", self.base + "^{tree}")
        b = self.git("commit-tree", tree, "-p", self.base, "-m", "b")
        c = self.git("commit-tree", tree, "-p", self.base, "-m", "c")
        first = self.git("commit-tree", tree, "-p", b, "-p", c, "-m", "first")
        second = self.git("commit-tree", tree, "-p", c, "-p", b, "-m", "second")
        self.assertEqual(len(self.git("merge-base", "--all", first, second).splitlines()), 2)
        result = SCOPE.classify(first, second, "pull_request")
        self.assertFalse(result["documentation_only"])
        self.assertIn("Ambiguous merge bases", result["reason"])

    def test_document_integrity_rejects_oversize_invalid_utf8_and_broken_local_links(self):
        for body in [b"x" * 20_001, b"\xff", b"[Broken](../missing.rs)", b"[Escape](../../outside)"]:
            with self.subTest(body=body[:30]):
                Path(DOC).write_bytes(body)
                head = self.commit()
                with self.assertRaises((ValueError, UnicodeError, subprocess.CalledProcessError)):
                    SCOPE.validate_documents(head, [DOC])


if __name__ == "__main__":
    unittest.main()
