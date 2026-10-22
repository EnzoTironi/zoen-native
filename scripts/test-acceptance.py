#!/usr/bin/env python3
"""Canonical acceptance composition and source-writer contracts; no app services."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).with_name("acceptance.py")
SPEC = importlib.util.spec_from_file_location("acceptance", SCRIPT)
acceptance = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(acceptance)


class AcceptanceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.manifest = self.root / "acceptance.json"
        self.parts = self.root / "acceptance/parts"
        self.parts.mkdir(parents=True)
        self.write(self.manifest, {"first": "unchanged", "packet": {"$part": "packet.json"}, "last": False})
        self.write(self.parts / "packet.json", {"text": " line\n  café 👩🏽‍💻 é ", "rows": [3, 1, 3]})

    def write(self, path, value):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    def load(self):
        return acceptance.load_acceptance(self.manifest)

    def tree(self):
        return {str(p.relative_to(self.root)): (p.read_bytes(), p.stat().st_mode & 0o777)
                for p in self.root.rglob("*") if p.is_file()}

    def cli(self, *args):
        return subprocess.run([sys.executable, str(SCRIPT), "--manifest", str(self.manifest), *args],
                              capture_output=True, text=True)

    def test_canonical_record_keeps_required_scope_and_incomplete_acceptance(self):
        document = acceptance.load_acceptance()
        self.assertEqual(len(document["requiredJourneys"]), 30)
        self.assertEqual(len(set(document["requiredJourneys"])), 30)
        self.assertEqual(document["platforms"], ["ios", "web", "android", "macos", "windows", "linux"])
        self.assertEqual(len(document["creationWorkspace"]["requiredCapabilities"]), 10)
        self.assertEqual(len(document["knownParityGaps"]), 10)
        for name in ["fullAppParityAccepted", "productionReadinessAccepted", "scaleReadinessAccepted"]:
            self.assertIs(document[name], False)
        self.assertEqual(document["historicalEvidence"]["commit"], "e76c74be4af4bfe483f4de4b63b6f932a83f0d8b")
        self.assertEqual(len(document["evidence"]), 27)
        self.assertEqual(len(document["postMergeCheckpoint"]), 27)
        self.assertEqual(len(document["deckcraftIntegrationCheckpoint"]), 30)

    def test_nested_parts_preserve_all_strings_list_and_object_order(self):
        packet = acceptance.read_json(self.parts / "packet.json")
        self.write(self.parts / "nested.json", [packet, "second", None, True, 9])
        self.write(self.parts / "packet.json", {"nested": {"$part": "nested.json"}, "tail": " "})
        expected = {"first": "unchanged", "packet": {"nested": [packet, "second", None, True, 9], "tail": " "}, "last": False}
        actual = self.load()
        self.assertEqual(json.dumps(actual, ensure_ascii=False), json.dumps(expected, ensure_ascii=False))
        self.assertEqual(actual, self.load())

    def test_missing_part_is_an_error_and_cli_prints_no_partial_document(self):
        (self.parts / "packet.json").unlink()
        with self.assertRaisesRegex(acceptance.AcceptanceError, "Cannot read"):
            self.load()
        result = self.cli("read")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")

    def test_duplicate_part_is_refused_even_through_two_distinct_containers(self):
        self.write(self.manifest, {"a": [{"$part": "packet.json"}], "b": {"$part": "packet.json"}})
        with self.assertRaisesRegex(acceptance.AcceptanceError, "Duplicate part"):
            self.load()

    def test_cycle_is_distinguished_from_duplicate_part(self):
        self.write(self.parts / "packet.json", {"$part": "nested.json"})
        self.write(self.parts / "nested.json", {"$part": "packet.json"})
        with self.assertRaisesRegex(acceptance.AcceptanceError, "Cyclic part"):
            self.load()

    def test_duplicate_json_keys_and_non_json_constants_are_refused(self):
        for path in [self.manifest, self.parts / "packet.json"]:
            original = path.read_bytes()
            for content in ['{"accepted":false,"accepted":true}', '{"n":NaN}', '{"n":Infinity}', '{"n":1e400}']:
                with self.subTest(path=path.name, content=content):
                    path.write_text(content)
                    with self.assertRaises(acceptance.AcceptanceError):
                        self.load()
            path.write_bytes(original)

    def test_reference_shape_and_canonical_relative_paths_are_strict(self):
        for ref in [{"$part": "packet.json", "override": True}, {"$part": 7},
                    {"$part": "../outside.json"}, {"$part": "/outside.json"},
                    {"$part": "a/../packet.json"}, {"$part": "a//packet.json"},
                    {"$part": "a\\packet.json"}, {"$part": "packet.txt"}]:
            with self.subTest(ref=ref):
                self.write(self.manifest, {"packet": ref})
                with self.assertRaises(acceptance.AcceptanceError):
                    self.load()

    def test_symlink_and_orphan_parts_are_not_an_alternate_authority(self):
        extra = self.parts / "extra.json"
        self.write(extra, {"hidden": True})
        with self.assertRaisesRegex(acceptance.AcceptanceError, "Unreferenced"):
            self.load()
        extra.unlink()
        original = self.parts / "packet.json"
        outside = self.root / "outside.json"
        original.rename(outside)
        original.symlink_to(outside)
        with self.assertRaisesRegex(acceptance.AcceptanceError, "symlink"):
            self.load()

    def test_writer_roundtrip_preserves_unicode_order_mode_and_other_parts(self):
        part = self.parts / "packet.json"
        part.chmod(0o640)
        manifest = self.manifest.read_bytes()
        value = {"text": "  edição\n👩🏽‍💻 é ", "rows": ["same", "first", "same"], "empty": []}
        acceptance.write_part("packet.json", value, self.manifest)
        self.assertEqual(self.load()["packet"], value)
        self.assertEqual(self.manifest.read_bytes(), manifest)
        self.assertEqual(part.stat().st_mode & 0o777, 0o640)
        acceptance.write_part("packet.json", value, self.manifest)
        self.assertEqual(self.load()["packet"], value)

    def test_invalid_writes_leave_every_existing_file_unchanged(self):
        before = self.tree()
        for name, value in [("unknown.json", {}), ("../outside.json", {}),
                            ("packet.json", {"$part": "missing.json"}),
                            ("packet.json", {"$part": "packet.json"}),
                            ("packet.json", {"text": "😀" * 6000}),
                            ("packet.json", {"n": float("nan")})]:
            with self.subTest(name=name, value_type=type(value).__name__):
                with self.assertRaises(ValueError):
                    acceptance.write_part(name, value, self.manifest)
                self.assertEqual(self.tree(), before)

    def test_atomic_replace_failure_preserves_source_and_removes_temporary_file(self):
        before = self.tree()
        with mock.patch.object(acceptance.os, "replace", side_effect=OSError("synthetic replace refusal")):
            with self.assertRaisesRegex(OSError, "synthetic"):
                acceptance.write_part("packet.json", {"text": "new"}, self.manifest)
        self.assertEqual(self.tree(), before)

    def test_cli_read_check_and_explicit_part_update_share_the_loader(self):
        before = self.load()
        result = self.cli("read")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), before)
        self.assertEqual(self.cli("check").returncode, 0)
        incoming = self.root / "incoming.json"
        self.write(incoming, {"text": "explicit update", "rows": [2, 1]})
        result = self.cli("update-part", "packet.json", "--from-json", str(incoming))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.load()["packet"], json.loads(incoming.read_text()))


if __name__ == "__main__":
    unittest.main(verbosity=2)
