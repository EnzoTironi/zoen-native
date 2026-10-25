"""Read actual blank-document browser exports with an independent PDF parser."""
import argparse
import json
from pathlib import Path
from pypdf import PdfReader

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
output = parser.parse_args().output.resolve(strict=True)
assert json.loads((output / "result.json").read_text())["status"] == "PASS"
names = ["baseline", "edited", "undone", "redone", "reopened"]
readers = {name: PdfReader(output / f"{name}.pdf") for name in names}
texts = {name: "".join(page.extract_text() for page in reader.pages)
         for name, reader in readers.items()}
assert texts["baseline"] == texts["undone"] == ""
assert texts["edited"] == texts["redone"] == texts["reopened"] == "Created in Zoen"
for reader in readers.values():
    assert len(reader.pages) == 1
    assert reader.pages[0].mediabox == readers["baseline"].pages[0].mediabox
    assert reader.pages[0].get("/Rotate", 0) == 0
assert (output / "reopened.pdf").read_bytes() == (output / "redone.pdf").read_bytes()
(output / "independent-reader.json").write_text(json.dumps({
    "status": "PASS", "reader": "pypdf", "pages": 1, "text": texts,
    "scope": "Actual blank PDF touch controls/save/native Undo/Redo/cold; synthetic canonical CAS only",
    "exactColdExport": True,
}, indent=2) + "\n")
print("PASS: blank PDF added text, native Undo/Redo and exact cold chosen export")
