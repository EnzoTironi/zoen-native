#!/usr/bin/env python3
"""Execute a source-qualified real engine/App probe and independently read its PDFs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def image_stream(reader):
    objects = reader.pages[0]["/Resources"]["/XObject"]
    images = [reference.get_object() for reference in objects.values()
              if reference.get_object().get("/Subtype") == "/Image"]
    if len(images) != 1:
        raise ValueError("Expected exactly one actual image XObject")
    return images[0]


def run(args):
    from pypdf import PdfReader, __version__ as reader_version
    if args.output.resolve().is_relative_to(Path(__file__).resolve().parents[3]):
        raise ValueError("Use a runtime output directory outside the repository")
    binary = args.binary.resolve(strict=True)
    if not binary.is_file() or args.binary.is_symlink():
        raise ValueError("Use a regular explicitly selected compiled binary")
    qualification = json.loads(args.receipt.read_text())
    binary_sha = sha(binary)
    if qualification.get("status") != "PASS" or not any(
        artifact.get("path") == str(binary) and artifact.get("sha256") == binary_sha
        for artifact in qualification.get("artifacts", [])
    ):
        raise ValueError("The binary does not match a successful source qualification")
    if args.output.exists() or args.output.is_symlink():
        raise ValueError("Runtime output must be a new directory")
    jpeg = args.jpeg.resolve(strict=True)
    if not jpeg.is_file() or args.jpeg.is_symlink():
        raise ValueError("The generated contributor JPEG must be a regular explicit input")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    command = [str(binary), str(args.output), str(jpeg)]
    measured = sys.platform == "darwin"
    if measured:
        command = ["/usr/bin/time", "-l", *command]
    env = dict(os.environ, PDFCRAFT_SYSTEM_FONTS="0")
    process = subprocess.run(command, env=env, capture_output=True, timeout=60)
    if process.returncode:
        raise ValueError("The actual engine/App probe failed: " + process.stderr.decode(errors="replace")[:2000])
    result = json.loads((args.output / "result.json").read_text())
    if not result["actualApp"]["executed"] or not result["largerCases"]["jpegDocument"]["executed"]:
        raise ValueError("The actual editor or JPEG path was not executed")
    original = PdfReader(args.output / "source.pdf", strict=True)
    paragraph = PdfReader(args.output / "paragraph-edited.pdf", strict=True)
    text = paragraph.pages[0].extract_text()
    replacement = "Human and agent approved this shared PDF."
    if replacement not in text or "Human and agent edits share one PDF." in text:
        raise ValueError("Independent reader did not recover the exact edited paragraph")
    opaque = "Opaque publisher data must survive save, Undo and recovery."
    for reader in (original, paragraph, PdfReader(args.output / "reviewed.pdf", strict=True)):
        if str(reader.trailer["/Root"]["/ZoenConformancePayload"]) != opaque:
            raise ValueError("The unknown catalog value was lost or changed")
    long = PdfReader(args.output / "review-32-pages.pdf", strict=True)
    if len(long.pages) != 32 or long.pages[-1].rotation != 90:
        raise ValueError("Independent page count/rotation did not match")
    image = image_stream(PdfReader(args.output / "original-image.pdf", strict=True))
    if (image["/Width"], image["/Height"]) != (1600, 1200):
        raise ValueError("Independent raster image dimensions changed")
    before = PdfReader(args.output / "original-jpeg.pdf", strict=True)
    after = PdfReader(args.output / "rotated-jpeg.pdf", strict=True)
    if image_stream(before).get_data() != jpeg.read_bytes() or image_stream(after).get_data() != jpeg.read_bytes():
        raise ValueError("JPEG encoded bytes changed during actual PDF save")
    if after.pages[0].rotation != 90:
        raise ValueError("Independent JPEG page rotation did not match")
    (args.output / "stdout.log").write_bytes(process.stdout)
    (args.output / "timing.log").write_bytes(process.stderr)
    proof = {"status": "PASS", "binarySha256": binary_sha, "qualificationSha256": sha(args.receipt),
             "reader": "pypdf", "readerVersion": reader_version, "jpegSha256": sha(jpeg),
             "paragraphExact": True, "opaqueCatalogPreserved": True, "longPages": 32,
             "lastPageRotation": 90, "jpegEncodedBytesPreserved": True,
             "measuredNativeMacRuntime": measured, "nativeIphoneAcceptance": False,
             "pdfHashes": {path.name: sha(path) for path in sorted(args.output.glob("*.pdf"))}}
    (args.output / "independent-reader.json").write_text(json.dumps(proof, indent=2) + "\n")
    print(json.dumps(proof))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("binary", "receipt", "output", "jpeg"):
        parser.add_argument("--" + name, type=Path, required=True)
    try:
        run(parser.parse_args())
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        parser.exit(1, str(error) + "\n")


if __name__ == "__main__":
    main()
