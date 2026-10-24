"""Recipe refusal and provenance tests; these do not claim Rust/worker execution."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

RECIPE = Path(__file__).resolve().parents[1] / "prepare-engine.py"
spec = importlib.util.spec_from_file_location("wordcraft_prepare", RECIPE)
recipe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recipe)


class Preparation(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.tree = Path(self.temp.name)
        self.git("init", "-q")
        self.git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                 "commit", "--allow-empty", "-qm", "fixture")
        self.revision = self.git("rev-parse", "HEAD").strip()

    def tearDown(self):
        self.temp.cleanup()

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.tree), *args], text=True)

    def test_unpinned_tree_is_unchanged(self):
        with self.assertRaisesRegex(ValueError, "unpinned"):
            recipe.prepare(self.tree)
        self.assertFalse((self.tree / "crates").exists())

    def test_dirty_tree_is_unchanged(self):
        (self.tree / "unreviewed").write_text("source")
        with patch.object(recipe, "REVISION", self.revision):
            with self.assertRaisesRegex(ValueError, "dirty"):
                recipe.prepare(self.tree)
        self.assertFalse((self.tree / "crates").exists())

    def test_copies_genuine_bytes_and_refuses_reentry(self):
        with patch.object(recipe, "REVISION", self.revision):
            result = recipe.prepare(self.tree)
            for name, digest in result["inputs"].items():
                source = recipe.ROOT / name
                self.assertEqual(recipe.hashlib.sha256(source.read_bytes()).hexdigest(), digest)
                if "host-bridge/" in name:
                    leaf = name.split("host-bridge/", 1)[1]
                    self.assertEqual(source.read_bytes(), (self.tree / "crates/zoen-host" / leaf).read_bytes())
            self.assertFalse(result["compiled"])
            self.assertFalse(result["runtimeExecuted"])
            self.assertFalse(result["appAvailable"])
            with self.assertRaises(ValueError):
                recipe.prepare(self.tree)


if __name__ == "__main__":
    unittest.main()
