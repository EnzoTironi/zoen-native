#!/usr/bin/env python3
"""Create original deterministic conformance media, with its own provenance receipt."""
import argparse
import hashlib
import json
from pathlib import Path
from PIL import Image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    output = args.output
    if output.resolve().is_relative_to(Path(__file__).resolve().parents[3]):
        parser.exit(1, "Use a media output directory outside the repository\n")
    receipt = output.with_suffix(".provenance.json")
    if output.exists() or output.is_symlink() or receipt.exists() or receipt.is_symlink():
        parser.exit(1, "Use new output paths\n")
    output.parent.mkdir(parents=True, exist_ok=True)
    image = Image.new("RGB", (1600, 1200))
    image.putdata([((20 if (x // 100 + y // 100) % 2 == 0 else 220), x % 256, y % 256)
                   for y in range(1200) for x in range(1600)])
    image.save(output, format="JPEG", quality=85)
    proof = {"authorship": "Original contributor pattern from the Zoen conformance generator",
             "source": str(Path(__file__).resolve()), "sourceSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
             "dimensions": [1600, 1200], "encoder": "Pillow JPEG quality85",
             "bytes": output.stat().st_size, "sha256": hashlib.sha256(output.read_bytes()).hexdigest()}
    receipt.write_text(json.dumps(proof, indent=2) + "\n")
    print(json.dumps(proof))


if __name__ == "__main__":
    main()
