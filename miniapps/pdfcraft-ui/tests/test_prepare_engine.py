"""Preparation safety only; these Git fixtures do not execute the PDF engine."""
import hashlib
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / "prepare-engine.py"
SPEC = importlib.util.spec_from_file_location("prepare_pdf", SOURCE)
PREP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PREP)


class PrepareTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.upstream = self.root / "upstream"
        self.upstream.mkdir()
        self.recipe = self.root / "recipe/miniapps/pdfcraft-ui"
        shutil.copytree(SOURCE.parent, self.recipe, ignore=shutil.ignore_patterns("__pycache__"))
        for name, value in {"HERE": self.recipe, "ROOT": self.root / "recipe",
                            "__file__": str(self.recipe / "prepare-engine.py")}.items():
            replacement = patch.object(PREP, name, value)
            replacement.start()
            self.addCleanup(replacement.stop)
        self.git("init", "-q")
        for name, value in [("user.name", "PDF fixture"), ("user.email", "fixture@example.invalid"),
                            ("commit.gpgsign", "false"), ("core.hooksPath", os.devnull),
                            ("gc.auto", "0"), ("maintenance.auto", "false")]:
            self.git("config", name, value)
        pins = "\n".join(f'{name}={{git="https://example.invalid/fixture",rev="{revision}"}}'
                         for name, revision in PREP.CONFIG["dependencyPins"].items())
        self.write("Cargo.toml", '[workspace]\nmembers=["crates/*"]\n[workspace.dependencies]\n' + pins)
        for name in PREP.CONFIG["sourceManifests"]:
            if name != "Cargo.toml":
                self.write(name, "pinned fixture\n")
        self.write("vendor/renderer/original.rs", "untouched upstream fixture\n")
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")
        config = dict(PREP.CONFIG)
        config["revision"] = self.git("rev-parse", "HEAD")
        config["sourceManifests"] = {name: hashlib.sha256((self.upstream / name).read_bytes()).hexdigest()
                                     for name in config["sourceManifests"]}
        replacement = patch.object(PREP, "CONFIG", config)
        replacement.start()
        self.addCleanup(replacement.stop)

    def git(self, *arguments):
        return subprocess.check_output(["git", "-C", str(self.upstream), *arguments], text=True,
                                       stderr=subprocess.PIPE).strip()

    def write(self, name, content):
        path = self.upstream / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)

    def refused(self):
        with self.assertRaises(ValueError):
            PREP.prepare(self.upstream)
        self.assertFalse((self.upstream / "crates/zoen-inspection").exists())
        self.assertFalse((self.upstream / "zoen-pdfcraft-source.json").exists())

    def test_exact_copies_preserve_upstream_and_do_not_claim_execution(self):
        names = self.git("ls-files").splitlines()
        before = {name: (self.upstream / name).read_bytes() for name in names}
        receipt = PREP.prepare(self.upstream)
        self.assertEqual(before, {name: (self.upstream / name).read_bytes() for name in names})
        for source in (self.recipe / "inspection").rglob("*"):
            if source.is_file():
                self.assertEqual(source.read_bytes(), (self.upstream / "crates/zoen-inspection" /
                                 source.relative_to(self.recipe / "inspection")).read_bytes())
        for status in ("profileChosen", "compiled", "runtimeExecuted", "appAvailable"):
            self.assertFalse(receipt[status])
        self.assertEqual(receipt["upstream"], PREP.CONFIG["revision"])
        with self.assertRaises(ValueError):
            PREP.prepare(self.upstream)

    def test_unpinned_or_dirty_source_produces_nothing(self):
        with patch.dict(PREP.CONFIG, {"revision": "0" * 40}):
            self.refused()
        self.write("untracked", "not clean")
        self.refused()

    def test_committed_manifest_drift_is_not_silently_accepted(self):
        self.write("Cargo.toml", '[workspace]\nmembers=[]\n')
        self.git("add", ".")
        self.git("commit", "-qm", "changed manifest")
        with patch.dict(PREP.CONFIG, {"revision": self.git("rev-parse", "HEAD")}):
            self.refused()

    def test_symlinks_and_generated_inputs_are_refused(self):
        source = self.recipe / "inspection/src/main.rs"
        source.unlink()
        source.symlink_to(SOURCE.parent / "inspection/src/main.rs")
        self.refused()
        source.unlink()
        source.write_bytes((SOURCE.parent / "inspection/src/main.rs").read_bytes())
        (self.recipe / "inspection/Cargo.lock").write_text("generated lock")
        self.refused()


if __name__ == "__main__":
    unittest.main()
