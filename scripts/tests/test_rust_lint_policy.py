"""The gate must catch new members and local opt-outs, not just today's manifests."""
from pathlib import Path
import runpy
import tempfile
import unittest

violations = runpy.run_path(
    str(Path(__file__).resolve().parents[1] / "check-rust-lint-policy.py")
)["violations"]

POLICY = '''
[workspace]
members = ["crates/*"]
[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
perf = { level = "warn", priority = -1 }
unwrap_used = "deny"
expect_used = "deny"
'''


class LintPolicyTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / "Cargo.toml").write_text(POLICY)
        self.member("first", '[lints]\nworkspace = true\n')

    def member(self, name, lint):
        directory = self.root / "crates" / name
        directory.mkdir(parents=True, exist_ok=True)
        (directory / "Cargo.toml").write_text(
            f'[package]\nname = "{name}"\nversion = "0.1.0"\n' + lint
        )

    def test_inherited_members_pass(self):
        self.assertEqual(violations(self.root), [])

    def test_new_member_must_inherit(self):
        self.member("new", "")
        errors = violations(self.root)
        self.assertEqual(len(errors), 1)
        self.assertIn("crates/new/Cargo.toml", errors[0])

    def test_local_override_cannot_silently_disable_policy(self):
        self.member("first", '[lints.clippy]\nunwrap_used = "allow"\n')
        self.assertTrue(violations(self.root))

    def test_disabled_shared_rule_is_rejected(self):
        (self.root / "Cargo.toml").write_text(
            POLICY.replace('expect_used = "deny"', 'expect_used = "allow"')
        )
        self.assertIn("expect_used", "\n".join(violations(self.root)))


if __name__ == "__main__":
    unittest.main()
