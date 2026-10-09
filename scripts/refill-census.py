#!/usr/bin/env python3
"""Summarize one isolated refill journey captured with ZOEN_NET_DEBUG=1."""

import argparse
import json
import re
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("log", type=Path)
args = parser.parse_args()
rows = {}
notices = []
markers = {
    "low_notices": "key packages low",
    "generated_batches": "key packages generated",
    "publish_replies": "key packages published",
}
for line in args.log.read_text(errors="replace").splitlines():
    if not any(marker in line for marker in markers.values()):
        continue
    match = re.search(r"\[(ana|bruno)\]", line)
    actor = match[1] if match else "unattributed"
    row = rows.setdefault(actor, dict.fromkeys(markers, 0))
    for field, marker in markers.items():
        row[field] += marker in line
    notices.append(line)
print(json.dumps({"actors": rows, "notice_lines": notices}, indent=2))
