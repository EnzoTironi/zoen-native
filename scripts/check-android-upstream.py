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

branches = ["main", "codex/native-android", "ux/audit-p1", "codex/desktop-web-chat-shell", "codex/roadmap-integration", "codex/backend-agent-proposals", "codex/backend-agent-ownership-proof", "codex/backend-model-runtime"]
open_prs = []
if args.fetch:
    open_prs = json.loads(subprocess.run(["gh", "pr", "list", "--repo", "EnzoTironi/zoen-native", "--state", "open", "--limit", "100", "--json", "number,title,headRefName,headRefOid,url,isDraft"], check=True, capture_output=True, text=True).stdout)
    pulls = [f"refs/pull/{pr['number']}/head:refs/remotes/origin/pr/{pr['number']}" for pr in open_prs]
    git("fetch", "origin", *branches, *pulls)
refs = {branch: git("rev-parse", "origin/" + branch).stdout.strip() for branch in branches}
head = git("rev-parse", "HEAD").stdout.strip()
# Updating these references requires reviewing the new Apple flows and their Android equivalents.
reviewed_ios = {
    "ux/audit-p1": "07a04de9434cd3e7d26ba30be00b5722360b9754",
    "codex/desktop-web-chat-shell": "39ccb765957f9a6e2f95c48aebfbd1b8a3aac38c",
    "codex/roadmap-integration": "d9ebb3ed0208c52ce84600078e933213b2cdc905",
}
ios_changes = {
    branch: git("diff", "--name-only", reviewed + ".." + refs[branch], "--", "apple", "miniapps").stdout.splitlines()
    for branch, reviewed in reviewed_ios.items()
}
def mask_rust(source):
    return re.sub(r'//[^\n]*|/\*.*?\*/|r(?P<hash>#+)".*?"(?P=hash)|"(?:\\.|[^"\\])*"',
                  lambda match: " " * len(match[0]), source, flags=re.S)

def body_end(masked, opening):
    depth = 1
    for index in range(opening + 1, len(masked)):
        depth += (masked[index] == "{") - (masked[index] == "}")
        if depth == 0:
            return index
    raise ValueError("Unclosed exported Rust definition")

def public_api(ref):
    methods, types = {}, {}
    paths = git("ls-tree", "-r", "--name-only", ref, "crates/roda-ffi/src").stdout.splitlines()
    for path in paths:
        if not path.endswith(".rs") or "tests" in Path(path).parts or path.endswith("/tests.rs"):
            continue
        source = git("show", ref + ":" + path).stdout
        masked = mask_rust(source)
        for exported in re.finditer(r"#\[uniffi::export(?:\([^\]]*\))?\]\s*impl\s+RodaEngine\s*\{", masked):
            opening = exported.end() - 1
            body = source[opening + 1:body_end(masked, opening)]
            for name, signature in re.findall(r"pub\s+(?:async\s+)?fn\s+(\w+)\s*(\([^{}]*?\)[^{}]*?)\{", body):
                if name in methods:
                    raise ValueError("Duplicate exported engine method: " + name)
                methods[name] = re.sub(r"\s+", "", signature)
        for record in re.finditer(r"#\[derive\([^\]]*uniffi::(?:Record|Enum|Error)[^\]]*\)\]\s*pub\s+(?:struct|enum)\s+(\w+)\s*\{", masked):
            opening = record.end() - 1
            body = masked[opening + 1:body_end(masked, opening)]
            types[record[1]] = hashlib.sha256(re.sub(r"\s+", "", body).encode()).hexdigest()
    return methods, types

facade, types = public_api(refs["main"])
android_facade, android_types = public_api(head)
candidate_facade, candidate_types = public_api(refs["codex/backend-agent-proposals"])
changed_signatures = [name for name, signature in facade.items() if candidate_facade.get(name) != signature]
changed_types = [name for name, shape in types.items() if candidate_types.get(name) != shape]
backend_work, unreviewed_interfaces, incompatible_candidates = [], [], []
for pr in open_prs:
    ref = f"origin/pr/{pr['number']}"
    commit = git("rev-parse", ref).stdout.strip()
    base = git("merge-base", refs["main"], ref).stdout.strip()
    changes = git("diff", "--name-only", base + ".." + ref).stdout.splitlines()
    integrated = git("merge-base", "--is-ancestor", ref, refs["main"], check=False).returncode == 0
    interface_changes = [path for path in changes if path.startswith(("apple/", "miniapps/"))]
    if pr["number"] != 41 and not integrated and interface_changes and reviewed_ios.get(pr["headRefName"]) != commit:
        unreviewed_interfaces.append({"pr": pr["url"], "commit": commit, "changed_paths": interface_changes})
    core_changes = [path for path in changes if path.startswith("crates/")]
    if pr["number"] == 41 or integrated or not core_changes:
        continue
    baseline, baseline_types = public_api(base)
    proposed, proposed_types = public_api(ref)
    changed = [name for name, value in baseline.items() if proposed.get(name) != value]
    changed_shapes = [name for name, value in baseline_types.items() if proposed_types.get(name) != value]
    requiring_review = [name for name in changed if proposed.get(name) != android_facade.get(name)]
    shapes_requiring_review = [name for name in changed_shapes if proposed_types.get(name) != android_types.get(name)]
    in_android = git("merge-base", "--is-ancestor", ref, head, check=False).returncode == 0
    candidate = {"pr": pr["url"], "title": pr["title"], "branch": pr["headRefName"], "commit": commit,
                 "merge_base": base, "integrated_in_android_source": in_android,
                 "status": "source merged into Android; runtime acceptance recorded separately" if in_android else "unmerged; runtime not integrated or validated by Android",
                 "changed_core_paths": core_changes, "changed_exported_functions": changed,
                 "changed_exported_types": changed_shapes, "functions_requiring_android_review": requiring_review,
                 "types_requiring_android_review": shapes_requiring_review,
                 "added_exported_functions": sorted(proposed.keys() - baseline.keys()),
                 "added_exported_types": sorted(proposed_types.keys() - baseline_types.keys())}
    backend_work.append(candidate)
    if requiring_review or shapes_requiring_review:
        incompatible_candidates.append(pr["url"])
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
    "open_pull_requests": open_prs,
    "backend_work_in_progress": backend_work,
    "unreviewed_interface_pull_requests": unreviewed_interfaces,
    "incompatible_backend_candidates": incompatible_candidates,
    "latest_main_is_ancestor": current_backend,
    "reviewed_ios_commits": reviewed_ios,
    "ios_changes_since_review": ios_changes,
    "android_public_api": {
        "exported_engine_functions": len(android_facade),
        "derived_records_enums_errors": len(android_types),
        "functions_added_to_main": sorted(android_facade.keys() - facade.keys()),
        "types_added_to_main": sorted(android_types.keys() - types.keys()),
        "existing_functions_changed_from_main": [name for name, signature in facade.items() if android_facade.get(name) != signature],
        "existing_types_changed_from_main": [name for name, shape in types.items() if android_types.get(name) != shape],
        "integration_requirement": "Retain the Android branch's shared-core additions when merging newer upstream work; an unmerged candidate does not yet contain them.",
    },
    "original_art_sources": sources,
    "ios_review_behaviors_adapted": ["unified inbox with kind filters", "preserve reading position", "count incoming root messages", "first unread excludes own and system events", "pinned apps outside scrolling history", "approval cards and list", "four approval gestures with a 4.5-second pre-commit undo window"],
    "pending_backend": {
        "pr": "https://github.com/EnzoTironi/zoen-native/pull/46",
        "commit": refs["codex/backend-agent-proposals"],
        "status": "unmerged candidate; recorded separately from the integrated backend",
        "public_facade_methods_compared": len(facade),
        "public_records_enums_errors_compared": len(types),
        "changed_public_facade_signatures": changed_signatures,
        "changed_public_record_enum_error_shapes": changed_types,
        "added_public_facade_methods": sorted(candidate_facade.keys() - facade.keys()),
        "added_public_types": sorted(candidate_types.keys() - types.keys()),
        "comparison_scope": "Static signatures of exported RodaEngine implementations and derived UniFFI records, enums and errors across the crate; callback and session implementations require review when their source changes.",
        "runtime_validation": "not integrated; signature compatibility does not prove candidate behavior",
        "changed_core_paths": git("diff", "--name-only", refs["main"] + ".." + refs["codex/backend-agent-proposals"], "--", "crates/roda-ffi").stdout.splitlines(),
    },
}
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({"report": str(args.output), "latest_main_is_ancestor": current_backend, "remote_heads": refs, "art_sources_match": all(value["matches"] for value in sources.values())}, indent=2))
if not current_backend or not all(value["matches"] for value in sources.values()) or any(ios_changes.values()) or changed_signatures or changed_types or unreviewed_interfaces or incompatible_candidates:
    raise SystemExit("Upstream changed: integrate main, review the changed Apple flows, verify original art and adapt incompatible facade signatures before claiming parity")
