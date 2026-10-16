#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import struct
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "evidence", Path(__file__).with_name("check-living-space-evidence.py"))
evidence = importlib.util.module_from_spec(spec)
spec.loader.exec_module(evidence)


class EvidenceInventoryTests(unittest.TestCase):
    def test_missing_captures_are_not_success(self):
        with tempfile.TemporaryDirectory() as directory:
            captures = evidence.inventory(Path(directory))
            self.assertEqual(len(captures), 90)
            self.assertTrue(all(c["status"] == "missing" for c in captures))

    def test_png_header_inventory_does_not_claim_behavior(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            # Minimal header fixture; this checks metadata parsing, not image validity.
            (root / "iphone-light-home.png").write_bytes(
                b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR"
                + struct.pack(">II", 440, 956))
            (root / "iphone-light-page.png").write_text("not a PNG")
            captures = {c["file"]: c for c in evidence.inventory(root)}
            home = captures["iphone-light-home.png"]
            self.assertEqual((home["status"], home["width"], home["height"]),
                             ("present", 440, 956))
            self.assertEqual(captures["iphone-light-page.png"]["status"], "invalid")
            self.assertEqual(captures["ipad-light-home.png"]["status"], "missing")


if __name__ == "__main__":
    unittest.main()
