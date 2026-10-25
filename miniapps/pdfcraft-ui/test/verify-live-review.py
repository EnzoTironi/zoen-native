"""Independent PDF semantic checks of actual live-review browser exports."""
import argparse
import hashlib
import json
from pathlib import Path
from pypdf import PdfReader

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
output = parser.parse_args().output.resolve(strict=True)
assert json.loads((output / "result.json").read_text())["status"] == "PASS"
names = ["baseline", "human", "proposal", "external", "undo-external", "undo-human", "reopened"]
readers = {name: PdfReader(output / f"{name}.pdf") for name in names}
texts = {name: [page.extract_text() for page in reader.pages] for name, reader in readers.items()}
baseline = readers["baseline"]
for name, reader in readers.items():
    assert len(reader.pages) == 2
    assert reader.trailer["/Root"]["/ZoenConformancePayload"] == baseline.trailer["/Root"]["/ZoenConformancePayload"]
    for before, after in zip(baseline.pages, reader.pages, strict=True):
        assert before.mediabox == after.mediabox
    assert texts[name][1] == texts["baseline"][1]
marker = " browser live review"
assert texts["human"][0].replace(marker, "") == texts["baseline"][0]
assert texts["human"][0].count(marker) == 1
assert texts["human"] == texts["proposal"] == texts["external"] == texts["undo-external"]
assert texts["baseline"] == texts["undo-human"] == texts["reopened"]
for name, reader in readers.items():
    assert reader.pages[0].get("/Rotate", 0) == 0
    assert reader.pages[1].get("/Rotate", 0) == (90 if name in ["proposal", "external"] else 0)
assert (output / "proposal.pdf").read_bytes() == (output / "external.pdf").read_bytes()
assert (output / "undo-human.pdf").read_bytes() == (output / "reopened.pdf").read_bytes()
(output / "independent-reader.json").write_text(json.dumps({
    "status": "PASS", "reader": "pypdf", "scope": "Real renderer/worker browser exports; no Core or iPhone claim",
    "exactText": texts, "pages": 2, "opaquePreserved": True, "rotationAndUndo": True,
    "sha256": {name: hashlib.sha256((output / f"{name}.pdf").read_bytes()).hexdigest() for name in names},
}, indent=2) + "\n")
print("PASS: human text, real external rotation, both native Undo levels, opaque data and exact cold export")
