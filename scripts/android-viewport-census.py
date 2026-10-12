#!/usr/bin/env python3
"""Compare native result JSONs before changing chat restoration behavior."""
import argparse
import json
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("results", nargs="+", type=Path)
arguments = parser.parse_args()
actors = []
for path in arguments.results:
    result = json.loads(path.read_text())
    actors.append({
        "source": str(path),
        "source_sha": result["source_sha"],
        "api": result["api"],
        "passed": result["passed"],
        "failed": result["failed"],
        "skipped": result["skipped"],
        "failures": [
            {"class": failure["class"], "method": failure["method"], "detail": failure["detail"]}
            for failure in result.get("failures", [])
        ],
    })
print(json.dumps({
    "premise": "Saved counters and one frame suffice to restore a chat viewport through subscription handover.",
    "actors": actors,
}, indent=2))
