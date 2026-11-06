"""Keep backend, Expo and documented Rust builds on the same pinned compiler."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]


class RustToolchainTests(unittest.TestCase):
    def test_backend_expo_and_documented_toolchains_match(self):
        backend = (ROOT / '.github/workflows/ci.yml').read_text()
        expo = (ROOT / '.github/workflows/expo.yml').read_text()
        readme = (ROOT / 'README.md').read_text()
        pin = re.findall(r"^  RUST_TOOLCHAIN: '([0-9]+\.[0-9]+\.[0-9]+)'$", backend, re.M)
        self.assertEqual(len(pin), 1, 'Backend CI needs exactly one explicit compiler pin')
        for label, source in [('Expo', expo), ('README', readme)]:
            versions = re.findall(r'rustup (?:toolchain install|default) ([0-9]+\.[0-9]+\.[0-9]+)', source)
            self.assertTrue(versions, f'{label} must retain its explicit toolchain setup')
            self.assertEqual(set(versions), set(pin), f'{label} compiler differs from backend CI')

    def test_documented_environment_override_matches_ci(self):
        backend = (ROOT / '.github/workflows/ci.yml').read_text()
        readme = (ROOT / 'README.md').read_text()
        pin = re.search(r"^  RUST_TOOLCHAIN: '([0-9]+\.[0-9]+\.[0-9]+)'$", backend, re.M)[1]
        overrides = re.findall(r'export RUSTUP_TOOLCHAIN=([0-9]+\.[0-9]+\.[0-9]+)', readme)
        self.assertTrue(overrides, 'Document the compiler used by local validation')
        self.assertEqual(set(overrides), {pin})

    def test_toolchain_regression_is_run_by_ci(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertTrue("python3 -m unittest discover -s scripts/tests -p 'test_ci_toolchain.py' -v" in workflow,
                        'CI must execute the compiler pin regression')


if __name__ == '__main__':
    unittest.main()
