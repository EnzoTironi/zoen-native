"""Reject oversized proposed blobs even if the author skipped the local hook."""
import shutil
import sys
import unittest

from hook_fixture import GitFixture, ROOT


class ChangedFileSizeTests(GitFixture, unittest.TestCase):
    def setUp(self):
        super().setUp()
        shutil.copyfile(ROOT / "scripts/check-changed-file-sizes.py",
                        self.repo / "scripts/check-changed-file-sizes.py")

    def commit(self):
        self.git("commit", "--quiet", "-m", "proposed change")
        return self.git("rev-parse", "HEAD").stdout.decode().strip()

    def check(self, base, head="HEAD", event="push"):
        return self.run_command(
            [sys.executable, "scripts/check-changed-file-sizes.py",
             "--base", base, "--head", head, "--event", event], check=False,
        )

    def test_unchanged_history_and_unproposed_index_are_ignored(self):
        self.write("historical.rs", b"x" * 20_001)
        base = self.commit()
        self.write("small.rs", b"small")
        self.commit()
        self.write("staged-not-proposed.rs", b"x" * 20_001)
        before = self.git("write-tree").stdout
        self.assertEqual(self.check(base).returncode, 0)
        self.assertEqual(self.git("write-tree").stdout, before)

    def test_exact_boundary_and_skipped_local_hook(self):
        self.write("seed.rs", b"seed")
        base = self.commit()
        self.write("large.rs", b"x" * 20_000)
        self.commit()
        self.assertEqual(self.check(base).returncode, 0)
        self.write("large.rs", b"x" * 20_001)
        self.commit()  # No hook is installed: CI must still reject the commit.
        result = self.check(base)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"20,001 bytes", result.stderr)
        self.assertIn(b"sibling modules", result.stderr)

    def test_renamed_authored_destination_is_checked(self):
        self.write("vendor/import.rs", b"x" * 20_001)
        base = self.commit()
        self.git("mv", "vendor/import.rs", "authored.rs")
        self.commit()
        self.assertIn(b"authored.rs", self.check(base).stderr)

    def test_lockfiles_codegen_and_binary_exclusions_are_shared(self):
        self.write("seed.rs", b"seed")
        base = self.commit()
        for name in ("Cargo.lock", "generated/types.rs", "photo.png"):
            self.write(name, b"x" * 20_001)
        self.commit()
        self.assertEqual(self.check(base).returncode, 0)

    def test_excalidraw_manifest_codegen_does_not_exempt_authored_neighbors(self):
        import json
        self.write("seed.rs", b"seed")
        base = self.commit()
        value = json.dumps({"fixture": "x" * 20_001}).encode()
        self.write("crates/miniapps/resources/excalidraw.manifest.json", value)
        self.commit()
        self.assertEqual(self.check(base).returncode, 0)
        neighbor = "crates/miniapps/resources/authored.manifest.json"
        self.write(neighbor, value)
        self.commit()
        self.assertIn(neighbor.encode(), self.check(base).stderr)

    def test_missing_base_fails_closed(self):
        self.write("seed.rs", b"seed")
        self.commit()
        result = self.check("missing-reference")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"Changed-file size check failed", result.stderr)

    def test_initial_push_compares_with_empty_tree(self):
        self.write("large.rs", b"x" * 20_001)
        self.commit()
        self.assertNotEqual(self.check("0" * 40).returncode, 0)

    def test_pr_excludes_target_only_changes_after_branch_divergence(self):
        self.write("historical.rs", b"x" * 20_001)
        ancestor = self.commit()
        self.git("checkout", "--quiet", "-b", "target")
        self.write("historical.rs", b"y" * 20_001)
        target = self.commit()
        self.git("checkout", "--quiet", "-b", "proposal", ancestor)
        self.write("small.rs", b"small")
        proposal = self.commit()
        self.assertEqual(self.check(target, proposal, "pull_request").returncode, 0)
        self.assertNotEqual(self.check(target, proposal, "push").returncode, 0)

    def test_ci_merge_snapshot_catches_combined_oversized_file(self):
        common = b"common\n" * 600
        self.write("combined.rs", common)
        ancestor = self.commit()
        self.git("checkout", "--quiet", "-b", "target")
        self.write("combined.rs", b"main\n" * 1700 + common)
        target = self.commit()
        self.git("checkout", "--quiet", "-b", "proposal", ancestor)
        self.write("combined.rs", common + b"proposal\n" * 945)
        proposal = self.commit()
        self.assertEqual(self.check(target, proposal, "pull_request").returncode, 0)
        self.git("checkout", "--quiet", "target")
        self.git("merge", "--quiet", "--no-edit", proposal)
        result = self.check(target, "HEAD", "pull_request")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"combined.rs", result.stderr)


if __name__ == "__main__":
    unittest.main()
