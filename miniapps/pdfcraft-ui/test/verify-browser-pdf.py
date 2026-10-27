"""Independent semantic checks of the real browser exports; never an engine stub."""
import argparse
import hashlib
import json
from pathlib import Path
from pypdf import PdfReader

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
args = parser.parse_args()
output = args.output.resolve(strict=True)
result = json.loads((output / "result.json").read_text())
if result["status"] != "PASS":
    raise ValueError("browser journey did not pass")
names = ["baseline", "typed", "undone", "redone", "reopened"]
readers = {name: PdfReader(output / f"{name}.pdf") for name in names}
texts = {name: [page.extract_text() for page in reader.pages] for name, reader in readers.items()}
base = readers["baseline"]
opaque = base.trailer["/Root"]["/ZoenConformancePayload"]
assert opaque == "Opaque publisher data must survive save, Undo and recovery."
for name, reader in readers.items():
    assert not reader.is_encrypted
    assert len(reader.pages) == len(base.pages) == 2
    assert reader.trailer["/Root"]["/ZoenConformancePayload"] == opaque
    assert texts[name][1] == texts["baseline"][1]
    for before, after in zip(base.pages, reader.pages, strict=True):
        assert before.mediabox == after.mediabox
        assert before.get("/Rotate", 0) == after.get("/Rotate", 0)
assert " browser touch" not in texts["baseline"][0]
assert texts["typed"][0].replace(" browser touch", "") == texts["baseline"][0]
assert texts["typed"][0].count(" browser touch") == 1
assert texts["undone"] == texts["baseline"]
assert texts["redone"] == texts["typed"] == texts["reopened"]
assert (output / "redone.pdf").read_bytes() == (output / "reopened.pdf").read_bytes()
receipt = {
    "status": "PASS", "reader": "pypdf", "scope": "Browser exports independently parsed; no Rust persistence or iPhone claim",
    "exactText": texts, "opaquePreserved": True, "pages": 2,
    "sha256": {name: hashlib.sha256((output / f"{name}.pdf").read_bytes()).hexdigest() for name in names},
}
(output / "independent-reader.json").write_text(json.dumps(receipt, indent=2) + "\n")
print("PASS: exact touch edit, real Undo/Redo content, opaque data, page geometry and exact cold export")
