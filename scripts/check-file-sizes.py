#!/usr/bin/env python3
"""Audit all tracked index blobs with the exact staged hook size/exclusion policy."""

import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="Emit the complete machine-readable report")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    os.chdir(root)
    policy = runpy.run_path(str(root / ".githooks/pre-commit"), run_name="size_policy")
    candidates = []
    for entry in policy["git"]("ls-files", "--stage", "-z").split(b"\0"):
        if not entry:
            continue
        metadata, path_bytes = entry.split(b"\t", 1)
        mode, oid, stage = metadata.split()
        if stage != b"0":
            raise ValueError("Resolve unmerged index entries before auditing file sizes")
        path = os.fsdecode(path_bytes)
        if mode in (b"100644", b"100755") and not policy["exempt"](path):
            candidates.append((path, oid))
    sizes = policy["object_sizes"](candidates)
    violations = []
    for path, oid in candidates:
        size = sizes[oid]
        if size > policy["MAX_BYTES"] and not policy["is_binary"](oid):
            violations.append({"path": path, "bytes": size, "tokens": size / 4,
                               "blob": oid.decode("ascii")})
    violations.sort(key=lambda row: (-row["bytes"], row["path"]))
    report = {"maxBytes": policy["MAX_BYTES"], "tokenEstimate": "bytes / 4",
              "scope": "all maintained tracked index blobs",
              "maintainedCandidates": len(candidates), "violationCount": len(violations),
              "violations": violations}
    if args.json:
        print(json.dumps(report, indent=2))
    else:
        for row in violations:
            print("{}: {:,} bytes = {:,.2f} tokens".format(
                ascii(row["path"]), row["bytes"], row["tokens"]))
        print("{} oversized files; {} maintained candidates; limit {} bytes.".format(
            len(violations), len(candidates), policy["MAX_BYTES"]))
    return 1 if violations else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print("File-size audit failed: " + str(error), file=sys.stderr)
        sys.exit(2)
