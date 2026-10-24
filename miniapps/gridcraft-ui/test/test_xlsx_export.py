"""File-only regressions using unchanged exports from the actual Grid engine."""

from copy import deepcopy
from hashlib import sha256
from io import BytesIO
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import warnings
from zipfile import BadZipFile, ZIP_DEFLATED, ZipFile

import xlsx_export as cli
import xlsx_reader as reader

FIXTURES = Path(__file__).parent / "fixtures"
TOUCH = (FIXTURES / "touch-export.xlsx").read_bytes()
FORMULA = (FIXTURES / "formula-export.xlsx").read_bytes()
EXPECTED = {"A1": {"value": "11"}, "B2": {"value": "22"}}


def changed(data=TOUCH, replacements=None, extra=()):
    replacements = replacements or {}
    output = BytesIO()
    with ZipFile(BytesIO(data)) as source, ZipFile(output, "w", ZIP_DEFLATED) as target:
        for name in source.namelist():
            value = replacements.get(name, source.read(name))
            target.writestr(name, value)
        for name, value in extra:
            target.writestr(name, value)
    return output.getvalue()


def sheet_changed(before, after):
    name = "xl/worksheets/sheet1.xml"
    with ZipFile(BytesIO(TOUCH)) as source:
        return changed(replacements={name: source.read(name).replace(before, after)})


class ExportTests(unittest.TestCase):
    def test_sealed_export_bytes_and_cells(self):
        provenance = json.loads((FIXTURES / "exports.json").read_text())
        for name, entry in provenance["files"].items():
            actual = (FIXTURES / name).read_bytes()
            self.assertEqual(len(actual), entry["bytes"])
            self.assertEqual(sha256(actual).hexdigest(), entry["sha256"])
        result = reader.inspect_bytes(TOUCH)
        proof = cli.verify(result, EXPECTED)
        self.assertFalse(proof["identityBindingVerified"])
        self.assertFalse(proof["uiJourneyVerified"])
        self.assertIsNone(proof["operatorContext"])

    def test_actual_formula_and_cached_result_both_required(self):
        result = reader.inspect_bytes(FORMULA)
        wanted = {"A1": {"value": "7"}, "A2": {"value": "14", "formula": "A1*2"}}
        cli.verify(result, wanted)
        for wrong in ({"A2": {"value": "14", "formula": "A1*3"}},
                      {"A2": {"value": "15", "formula": "A1*2"}},
                      {"A2": {"value": "14"}}):
            with self.assertRaises(ValueError):
                cli.verify(result, wrong)

    def test_current_other_file_cannot_pass_selected_export_digest(self):
        result = reader.inspect_bytes(TOUCH)
        metadata = {"format": "xlsx", "sha256": result["sha256"], "bytes": result["bytes"],
                    "byteLength": result["bytes"],
                    "sourceHead": {"version": "2", "sha256": sha256(FORMULA).hexdigest()}}
        # Synthetic *context* exercises comparison only, never Item association.
        context = {"itemId": "synthetic-test-context", "sourceHead": metadata["sourceHead"]}
        proof = cli.verify(result, EXPECTED, metadata, context)
        self.assertTrue(proof["exportMetadataMatched"])
        self.assertFalse(proof["identityBindingVerified"])
        self.assertEqual(proof["operatorContext"], context)
        for field, value in (("sha256", sha256(FORMULA).hexdigest()), ("bytes", 1),
                             ("byteLength", 1), ("format", "pdf")):
            wrong = deepcopy(metadata)
            wrong[field] = value
            with self.assertRaises(ValueError):
                cli.verify(result, EXPECTED, wrong, context)
        wrong = deepcopy(context)
        wrong["sourceHead"]["version"] = "3"
        with self.assertRaisesRegex(ValueError, "head mismatch"):
            cli.verify(result, EXPECTED, metadata, wrong)
        with self.assertRaises(ValueError):
            cli.verify(result, EXPECTED, context=context)

    def test_wrong_missing_or_non_numeric_cell_refuses(self):
        for wanted in ({"A1": {"value": "12"}}, {"B1": {"value": "22"}},
                       {"A1": {"value": "NaN"}}, {"A1": {"value": 11}},
                       {"A0": {"value": "11"}}):
            with self.assertRaises(ValueError):
                cli.verify(reader.inspect_bytes(TOUCH), wanted)
        data = sheet_changed(b'<c r="A1">', b'<c r="A1" t="s">')
        with self.assertRaisesRegex(ValueError, "numeric cell"):
            cli.verify(reader.inspect_bytes(data), EXPECTED)

    def test_bounds_before_any_inflation(self):
        samples = [("archive entry bound", changed(extra=[(f"extra/{i}", b"") for i in range(129)])),
                   ("part byte bound", changed(extra=[("large.xml", b"x" * (reader.MAX_PART + 1))])),
                   ("inflated total bound", changed(extra=[(f"part{i}", b"x" * reader.MAX_PART)
                                                           for i in range(4)]))]
        for message, data in samples:
            with self.subTest(message=message), patch.object(ZipFile, "open", side_effect=AssertionError("inflated")):
                with self.assertRaisesRegex(ValueError, message):
                    reader.inspect_bytes(data)
        with patch.object(reader, "ZipFile", side_effect=AssertionError("allocated ZIP")):
            with self.assertRaisesRegex(ValueError, "archive byte bound"):
                reader.inspect_bytes(b"x" * (reader.MAX_ARCHIVE + 1))

    def test_duplicate_path_traversal_and_symlink_refuse(self):
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", UserWarning)
            duplicate = changed(extra=[("xl/workbook.xml", b"other")])
        for data, reason in ((duplicate, "duplicate archive"),
                             (changed(extra=[("../outside.xml", b"x")]), "unsafe archive"),
                             (changed(extra=[("/outside.xml", b"x")]), "unsafe archive")):
            with self.assertRaisesRegex(ValueError, reason):
                reader.inspect_bytes(data)
        data = bytearray(TOUCH)
        central = data.index(b"PK\x01\x02")
        struct.pack_into("<I", data, central + 38, 0o120777 << 16)
        with self.assertRaisesRegex(ValueError, "non-file"):
            reader.inspect_bytes(data)

    def test_crc_damage_not_accepted_by_declared_hash(self):
        data = bytearray(TOUCH)
        # Corrupt the declared CRC for the first member, deterministically. A
        # changed padding bit in compressed data need not change decoded bytes.
        central = data.index(b"PK\x01\x02")
        crc = struct.unpack_from("<I", data, central + 16)[0]
        struct.pack_into("<I", data, central + 16, crc ^ 1)
        with self.assertRaises(BadZipFile):
            reader.inspect_bytes(data)

    def test_xml_entity_external_relationship_and_duplicates_refuse(self):
        samples = [(sheet_changed(b"<worksheet ", b'<!DOCTYPE x [<!ENTITY boom "x">]><worksheet '), "DTD"),
                   (sheet_changed(b'<c r="B2">', b'<c r="A1">'), "duplicate worksheet"),
                   (sheet_changed(b"<v>11</v>", b"<v>11</v><v>22</v>"), "ambiguous cell")]
        path = "xl/_rels/workbook.xml.rels"
        with ZipFile(BytesIO(TOUCH)) as archive:
            raw = archive.read(path).replace(b'Target="worksheets/sheet1.xml"',
                                             b'Target="https://example.invalid/x" TargetMode="External"')
        samples.append((changed(replacements={path: raw}), "external relationship"))
        for data, reason in samples:
            with self.subTest(reason=reason), self.assertRaisesRegex(ValueError, reason):
                reader.inspect_bytes(data)

    def test_sheet_selected_by_name_and_relationship_not_filename(self):
        with ZipFile(BytesIO(TOUCH)) as archive:
            workbook = archive.read("xl/workbook.xml").replace(b'name="Sheet1"', b'name="Chosen"')
            rels = archive.read("xl/_rels/workbook.xml.rels").replace(b"worksheets/sheet1.xml", b"worksheets/selected.xml")
            worksheet = archive.read("xl/worksheets/sheet1.xml")
        data = changed(replacements={"xl/workbook.xml": workbook,
                                     "xl/_rels/workbook.xml.rels": rels},
                       extra=[("xl/worksheets/selected.xml", worksheet)])
        cli.verify(reader.inspect_bytes(data, "Chosen"), EXPECTED)
        with self.assertRaisesRegex(ValueError, "selected sheet missing"):
            reader.inspect_bytes(data)

    def test_cli_existing_browser_shape_and_failure_exit(self):
        script = str(Path(cli.__file__).resolve())
        file = str(FIXTURES / "touch-export.xlsx")
        cells = subprocess.run([sys.executable, script, "cells", file], capture_output=True, check=True)
        self.assertEqual(json.loads(cells.stdout), {"A1": {"value": "11", "formula": ""},
                                                  "B2": {"value": "22", "formula": ""}})
        with tempfile.TemporaryDirectory() as directory:
            expected = Path(directory) / "expected.json"
            expected.write_text(json.dumps(EXPECTED))
            command = [sys.executable, script, "verify", file, "--expect", str(expected)]
            proof = subprocess.run(command, capture_output=True, check=True)
            self.assertEqual(json.loads(proof.stdout)["result"], "PASS")
            expected.write_text('{"A1":{"value":"0"}}')
            failed = subprocess.run(command, capture_output=True)
            self.assertEqual(failed.returncode, 1)
            self.assertFalse(failed.stdout)
            self.assertEqual(json.loads(failed.stderr)["result"], "FAIL")

    def test_input_records_are_bounded_unambiguous_and_not_authority(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "record.json"
            for raw, reason in ((b'{"x":1,"x":2}', "duplicate JSON"),
                                (b"x" * (64 * 1024 + 1), "JSON input byte bound")):
                path.write_bytes(raw)
                with self.assertRaisesRegex(ValueError, reason):
                    cli.read_json(path)
        result = reader.inspect_bytes(TOUCH)
        bad = {"format": "xlsx", "bytes": result["bytes"], "sha256": result["sha256"],
               "sourceHead": {"version": "02", "sha256": result["sha256"]}}
        with self.assertRaisesRegex(ValueError, "source version"):
            cli.verify(result, EXPECTED, bad)

    def test_malformed_archive_or_xml_cli_never_echoes_contents(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "bad.xlsx"
            script = str(Path(cli.__file__).resolve())
            for raw in (b"not-a-zip PRIVATE_SENTINEL", sheet_changed(b"</worksheet>", b"PRIVATE_SENTINEL")):
                path.write_bytes(raw)
                failed = subprocess.run([sys.executable, script, "cells", str(path)], capture_output=True)
                self.assertEqual(failed.returncode, 1)
                self.assertFalse(failed.stdout)
                self.assertEqual(json.loads(failed.stderr)["result"], "FAIL")
                self.assertNotIn(b"PRIVATE_SENTINEL", failed.stderr)


if __name__ == "__main__":
    unittest.main()
