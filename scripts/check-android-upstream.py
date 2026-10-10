#!/usr/bin/env python3
"""Validate Android's backend ancestry and record the live iOS review references."""
import argparse
import datetime
import gzip
import hashlib
import json
import re
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--fetch", action="store_true", help="Refresh the authoritative remote branches before checking")
parser.add_argument("--output", type=Path, default=root / "build/android-upstream.json")
args = parser.parse_args()

def git(*words, check=True):
    return subprocess.run(["git", *words], cwd=root, check=check, capture_output=True, text=True)

branches = ["main", "codex/native-android", "ux/audit-p1", "codex/desktop-web-chat-shell", "codex/backend-agent-proposals"]
if args.fetch:
    git("fetch", "origin", *branches)
refs = {branch: git("rev-parse", "origin/" + branch).stdout.strip() for branch in branches}
head = git("rev-parse", "HEAD").stdout.strip()
def public_facade(ref):
    source = git("show", ref + ":crates/roda-ffi/src/lib.rs").stdout
    return {name: re.sub(r"\s+", "", signature) for name, signature in re.findall(r"pub\s+(?:async\s+)?fn\s+(\w+)\s*(\([^{}]*?\)[^{}]*?)\{", source)}

facade = public_facade(refs["main"])
candidate_facade = public_facade(refs["codex/backend-agent-proposals"])
changed_signatures = [name for name, signature in facade.items() if candidate_facade.get(name) != signature]
current_backend = git("merge-base", "--is-ancestor", refs["main"], head, check=False).returncode == 0
sources = {}
for fixture in ["ink-swift-reference.json.gz", "ink-icons-swift-reference.json.gz"]:
    document = json.loads(gzip.decompress((root / "android/app/src/test/resources" / fixture).read_bytes()))
    for path, expected in document["sources"].items():
        hashes = {}
        for name in ["main", "ux/audit-p1", "codex/desktop-web-chat-shell"]:
            source = subprocess.run(["git", "show", refs[name] + ":" + path], cwd=root, capture_output=True, check=True).stdout
            hashes[name] = hashlib.sha256(source).hexdigest()
        sources[path] = {"fixture_sha256": expected, "upstream_sha256": hashes, "matches": all(value == expected for value in hashes.values())}

report = {
    "checked_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "android_checkout": head,
    "has_uncommitted_changes": bool(git("status", "--porcelain").stdout),
    "remote_heads": refs,
    "latest_main_is_ancestor": current_backend,
    "original_art_sources": sources,
    "ios_review_behaviors_adapted": ["unified inbox with kind filters", "preserve reading position", "count incoming root messages", "first unread excludes own and system events", "pinned apps outside scrolling history"],
    "pending_backend": {
        "pr": "https://github.com/EnzoTironi/zoen-native/pull/46",
        "commit": refs["codex/backend-agent-proposals"],
        "status": "unmerged candidate; recorded separately from the integrated backend",
        "public_facade_methods_compared": len(facade),
        "changed_public_facade_signatures": changed_signatures,
        "runtime_validation": "not integrated; signature compatibility does not prove candidate behavior",
        "changed_core_paths": git("diff", "--name-only", refs["main"] + ".." + refs["codex/backend-agent-proposals"], "--", "crates/roda-ffi").stdout.splitlines(),
    },
}
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({"report": str(args.output), "latest_main_is_ancestor": current_backend, "remote_heads": refs, "art_sources_match": all(value["matches"] for value in sources.values())}, indent=2))
if not current_backend or not all(value["matches"] for value in sources.values()):
    raise SystemExit("Upstream changed: integrate the current main or regenerate and verify the original art before claiming parity")
