"""Compile tiny dependency-free fixtures using the real shared Clippy policy."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class CompilerLintTests(unittest.TestCase):
    def compile(self, source):
        with tempfile.TemporaryDirectory(prefix="zoen-lint-contract-") as directory:
            root = Path(directory)
            policy = (ROOT / "Cargo.toml").read_text().split("[workspace.lints.rust]", 1)[1]
            (root / "Cargo.toml").write_text(
                '[workspace]\nmembers = ["fixture"]\nresolver = "2"\n'
                + "[workspace.lints.rust]" + policy
            )
            (root / "clippy.toml").write_text((ROOT / "clippy.toml").read_text())
            package = root / "fixture"
            (package / "src").mkdir(parents=True)
            (package / "Cargo.toml").write_text(
                '[package]\nname = "lint-contract"\nversion = "0.1.0"\n'
                'edition = "2021"\n[lints]\nworkspace = true\n'
            )
            (package / "src" / "lib.rs").write_text(source)
            env = os.environ.copy()
            # Never write this fixture into the shared product build cache.
            env["CARGO_TARGET_DIR"] = str(root / "target")
            env["CARGO_BUILD_BUILD_DIR"] = str(root / "target")
            result = subprocess.run(
                ["cargo", "clippy", "--offline", "--all-targets", "--message-format=json",
                 "--manifest-path", str(root / "Cargo.toml"), "--", "-D", "warnings"],
                cwd=root, env=env, text=True, capture_output=True, timeout=45, check=False,
            )
            codes = set()
            for line in result.stdout.splitlines():
                message = json.loads(line)
                if message.get("reason") == "compiler-message":
                    code = message["message"].get("code")
                    if code:
                        codes.add(code["code"])
            return result.returncode, codes, result.stderr

    def require_rejection(self, source, code):
        status, codes, stderr = self.compile(source)
        self.assertNotEqual(status, 0, stderr)
        self.assertIn(code, codes, stderr)

    def test_production_unwrap_is_rejected(self):
        self.require_rejection(
            '#[must_use]\npub fn read() -> String { std::env::var("FIXTURE").unwrap() }',
            "clippy::unwrap_used",
        )

    def test_production_expect_is_rejected(self):
        self.require_rejection(
            '#[must_use]\npub fn read() -> String { std::env::var("FIXTURE").expect("env") }',
            "clippy::expect_used",
        )

    def test_pedantic_contract_is_active(self):
        self.require_rejection(
            'pub fn nonempty(value: &str) -> bool { !value.is_empty() }',
            "clippy::must_use_candidate",
        )

    def test_performance_contract_is_active(self):
        self.require_rejection(
            '#[must_use]\npub fn copy(values: &[u8]) -> Vec<u8> { '
            'values.iter().copied().collect() }',
            "clippy::iter_cloned_collect",
        )

    def test_unused_result_is_rejected(self):
        self.require_rejection(
            'pub fn ignored() { std::fs::remove_file("fixture"); }',
            "unused_must_use",
        )

    def test_unsafe_operation_requires_explicit_block(self):
        self.require_rejection(
            '/// # Safety\n/// The pointer must be valid.\n'
            'pub unsafe fn read(value: *const u8) -> u8 { *value }',
            "E0133",
        )

    def test_unsafe_block_requires_documented_justification(self):
        self.require_rejection(
            '/// # Safety\n/// The pointer must be valid.\n'
            'pub unsafe fn read(value: *const u8) -> u8 { unsafe { *value } }',
            "clippy::undocumented_unsafe_blocks",
        )

    def test_test_function_assertions_are_distinct_from_production(self):
        status, codes, stderr = self.compile(
            '#[cfg(test)]\nmod tests {\n#[test]\nfn assertion() { '
            'let value = std::env::var("ASSERTION_ONLY").unwrap(); '
            'assert_eq!(value, "fixture"); } }'
        )
        self.assertEqual(status, 0, (codes, stderr))


if __name__ == "__main__":
    unittest.main()
