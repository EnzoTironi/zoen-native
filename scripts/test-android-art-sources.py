#!/usr/bin/env python3
"""Exercise corpus-bound art equivalence without an Android build or device."""

import gzip
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("android_upstream", Path(__file__).with_name("check-android-upstream.py"))
upstream = importlib.util.module_from_spec(spec)
spec.loader.exec_module(upstream)

HAND = "apple/Shared/DesignSystem/HandDrawn.swift"
MASCOT = "apple/Shared/DesignSystem/Mascot.swift"
ICON = "apple/Shared/DesignSystem/ZoenIcons.swift"
PET = "apple/Shared/Apps/Native/PetViews.swift"


class ArtSourceEquivalenceTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.resources = self.root / "android/app/src/test/resources"
        self.resources.mkdir(parents=True)
        self.original = {path: (path + " original").encode() for path in (HAND, MASCOT, ICON, PET)}
        self.main = dict(self.original)
        for path, content in self.original.items():
            target = self.root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
        pet = self.root / "android/app/src/main/java/xyz/tironi/zoen/ui/GeneratedPetArt.kt"
        pet.parent.mkdir(parents=True)
        pet.write_text('const val sourceSha256 = "' + upstream.core.digest(self.original[PET]) + '"')
        self.corpora = {
            "ink": {"frames": [{"time": 0.37}], "doodles": [], "geometry": {}, "rng": [0.5]},
            "ink-icons": {"icons": [{"name": "heart", "beats": [0.37]}]},
        }
        for name, corpus in self.corpora.items():
            paths = (HAND, MASCOT) if name == "ink" else (HAND, ICON)
            document = {**corpus, "sources": {path: upstream.core.digest(self.original[path]) for path in paths}}
            (self.resources / (name + "-swift-reference.json.gz")).write_bytes(gzip.compress(json.dumps(document).encode()))
            self.write_equivalence(name)
        patcher = patch.object(upstream.subprocess, "run", side_effect=self.run_command)
        patcher.start()
        self.addCleanup(patcher.stop)

    def write_equivalence(self, name, variants=None):
        corpus = self.corpora[name]["icons"] if name == "ink-icons" else self.corpora[name]
        key = "reference_icons_sha256" if name == "ink-icons" else "reference_corpus_sha256"
        paths = (HAND, MASCOT) if name == "ink" else (HAND, ICON)
        document = {key: upstream.core.json_digest(corpus),
                    "source_variants": {path: [upstream.core.digest(self.original[path])] for path in paths}}
        for path, content in (variants or {}).items():
            document["source_variants"][path].append(upstream.core.digest(content))
        (self.resources / (name + "-source-equivalence.json")).write_text(json.dumps(document))

    def run_command(self, command, **kwargs):
        if command[:2] == ["git", "show"]:
            return subprocess.CompletedProcess(command, 0, stdout=self.main[command[2].split(":", 1)[1]])
        return subprocess.CompletedProcess(command, 0)

    def check(self):
        return upstream.artwork(self.root, "fixture-main")["original_sources"]

    def change_hand(self):
        content = b"changed HandDrawn view wrapper"
        (self.root / HAND).write_bytes(content)
        return content

    def test_original_sources_pass_without_mascot_equivalence(self):
        (self.resources / "ink-source-equivalence.json").unlink()
        self.assertTrue(all(item["matches"] for item in self.check().values()))

    def test_shared_source_requires_both_corpora_to_verify_equivalence(self):
        content = self.change_hand()
        self.write_equivalence("ink-icons", {HAND: content})
        self.assertFalse(self.check()[HAND]["matches"])
        self.write_equivalence("ink", {HAND: content})
        self.assertTrue(self.check()[HAND]["matches"])

    def test_mascot_equivalence_alone_does_not_accept_shared_source(self):
        content = self.change_hand()
        self.write_equivalence("ink", {HAND: content})
        self.assertFalse(self.check()[HAND]["matches"])

    def test_unknown_upstream_source_is_rejected_even_when_snapshot_matches(self):
        self.main[ICON] = b"unverified upstream icon changes"
        self.assertFalse(self.check()[ICON]["matches"])

    def test_unknown_snapshot_source_is_rejected(self):
        (self.root / ICON).write_bytes(b"unverified snapshot icon changes")
        self.assertFalse(self.check()[ICON]["matches"])

    def test_equivalence_cannot_drop_original_fixture_hash(self):
        target = self.resources / "ink-icons-source-equivalence.json"
        document = json.loads(target.read_text())
        document["source_variants"][ICON] = [upstream.core.digest(b"new source")]
        target.write_text(json.dumps(document))
        with self.assertRaisesRegex(ValueError, "retain the original fixture"):
            self.check()

    def test_changed_corpus_rejects_stale_equivalence(self):
        for name in self.corpora:
            with self.subTest(corpus=name):
                target = self.resources / (name + "-swift-reference.json.gz")
                original = target.read_bytes()
                document = json.loads(gzip.decompress(original))
                document["icons" if name == "ink-icons" else "frames"] = []
                target.write_bytes(gzip.compress(json.dumps(document).encode()))
                with self.assertRaisesRegex(ValueError, "does not match the corpus"):
                    self.check()
                target.write_bytes(original)


if __name__ == "__main__":
    unittest.main()
