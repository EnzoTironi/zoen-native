"""Actual Git fixtures exercise preparation; these do not execute PhotoCraft."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / "prepare-engine.py"
SPEC = importlib.util.spec_from_file_location("prepare_photo", SOURCE)
PREP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PREP)


class PrepareTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.upstream = self.root / "upstream"
        self.upstream.mkdir()
        self.recipe = self.root / "recipe/miniapps/photocraft-ui"
        shutil.copytree(SOURCE.parent, self.recipe, ignore=shutil.ignore_patterns("__pycache__"))
        for key, value in {"HERE": self.recipe, "ROOT": self.root / "recipe",
                           "__file__": str(self.recipe / "prepare-engine.py")}.items():
            applied = patch.object(PREP, key, value)
            applied.start()
            self.addCleanup(applied.stop)
        self.git("init", "-q")
        for key, value in [("user.name", "Photo fixture"), ("user.email", "fixture@example.invalid"),
                           ("commit.gpgsign", "false"), ("core.hooksPath", os.devnull),
                           ("gc.auto", "0"), ("maintenance.auto", "false")]:
            self.git("config", key, value)
        pins = "\n".join(f'{name}={{git="https://example.invalid/fixture",rev="{revision}"}}'
                         for name, revision in PREP.CONFIG["dependencyPins"].items())
        self.write("Cargo.toml", '[workspace]\nmembers=["crates/*"]\n[workspace.dependencies]\n' + pins)
        for name in PREP.SOURCE_MANIFESTS[1:]:
            self.write(name, '[package]\nname="fixture"\nversion="0.1.0"\n')
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")
        self.head = self.git("rev-parse", "HEAD")
        applied = patch.object(PREP, "REVISION", self.head)
        applied.start()
        self.addCleanup(applied.stop)

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.upstream), *args], text=True,
                                       stderr=subprocess.PIPE).strip()

    def write(self, name, text):
        p = self.upstream / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)

    def refused(self):
        with self.assertRaises(ValueError):
            PREP.prepare(self.upstream)
        self.assertFalse((self.upstream / "zoen-photocraft-source.json").exists())
        self.assertFalse((self.upstream / "crates/zoen-host").exists())

    def test_copies_exact_inputs_and_preserves_all_original_upstream_bytes(self):
        before = {p: (self.upstream / p).read_bytes() for p in PREP.SOURCE_MANIFESTS}
        result = PREP.prepare(self.upstream)
        self.assertFalse(result["compiled"])
        self.assertFalse(result["runtimeExecuted"])
        self.assertFalse(result["appAvailable"])
        self.assertEqual(result["upstream"], self.head)
        for relative in ["Cargo.toml", "src/lib.rs", "src/archive.rs", "src/tests.rs"]:
            self.assertEqual((self.upstream / "crates/zoen-host" / relative).read_bytes(),
                             (self.recipe / "host-bridge" / relative).read_bytes())
        self.assertEqual(before, {p: (self.upstream / p).read_bytes() for p in before})
        with self.assertRaises(ValueError):
            PREP.prepare(self.upstream)

    def test_unpinned_and_dirty_source_are_rejected_before_output(self):
        with patch.object(PREP, "REVISION", "0" * 40):
            self.refused()
        self.write("untracked", "not a clean owned input")
        self.refused()

    def test_symlinked_adapter_and_upstream_directory_are_rejected(self):
        link = self.root / "linked-upstream"
        link.symlink_to(self.upstream, target_is_directory=True)
        with self.assertRaises(ValueError):
            PREP.prepare(link)
        source = self.recipe / "host-bridge/src/lib.rs"
        source.unlink()
        source.symlink_to(SOURCE.parent / "host-bridge/src/lib.rs")
        self.refused()

    def test_dependency_or_workspace_drift_cannot_produce_a_receipt(self):
        manifest = self.upstream / "Cargo.toml"
        manifest.write_text(manifest.read_text().replace('members=["crates/*"]', 'members=[]'))
        self.git("add", ".")
        self.git("commit", "-qm", "drift")
        with patch.object(PREP, "REVISION", self.git("rev-parse", "HEAD")):
            self.refused()

    def test_new_ui_dependency_requires_review_instead_of_silent_activation(self):
        manifest = self.recipe / "host-bridge/Cargo.toml"
        manifest.write_text(manifest.read_text().replace('[dependencies]',
                            '[dependencies]\neframe={version="0.36"}'))
        self.refused()

    def test_generated_lockfile_or_targets_are_not_recipe_inputs(self):
        (self.recipe / "host-bridge/Cargo.lock").write_text("not an authored source")
        self.refused()


if __name__ == "__main__":
    unittest.main()
