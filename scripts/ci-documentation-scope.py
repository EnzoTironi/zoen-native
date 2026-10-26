#!/usr/bin/env python3
"""Select reviewed prose/frontend PR lanes from immutable Git; unknown means full."""

import argparse
import hashlib
import json
import os
from pathlib import PurePosixPath
import re
import subprocess
import sys
from urllib.parse import unquote, urlsplit


# Exact human-maintained prose, not a blanket *.md exception. Adding an entry
# changes this script and therefore requires the complete suite on that PR.
DOCUMENTS = frozenset({
    "docs/agent-runtime-capabilities.md",
    "docs/agent-runtime-capabilities-execution.md",
    "docs/product/telegram-ios-conversation-implementation.md",
    "docs/product/telegram-ios-conversation-implementation-batches.md",
})
SELF = "scripts/ci-documentation-scope.py"
TEST = "scripts/tests/test_ci_documentation_scope.py"
DISJOINT_TEST = "scripts/tests/test_ci_frontend_disjoint_scope.py"

# This unchanged JS fixture reads the child App layout, not the ordinary root
# layout. Admit only this reviewed reader/path pair; a basename collision is
# not permission to ignore another changed input read by the same fixture.
DISJOINT_FRONTEND_REFERENCES = {
    ("miniapps/excalidraw-ui/test/native-host-navigation-fixture.mjs",
     "apps/zoen/src/app/_layout.tsx"): (
        "177d00b51ff5669e9ccd4f16c0853890c9e2115472b97fd926ebf12b887fb902",
        "apps/zoen/src/app/miniapp/_layout.tsx",
    ),
}

from ci_frontend_inputs import (
    FRONTEND_CONTRACTS, NON_INPUT_APP_LITERALS, RESOURCE_MANIFESTS, RUST_ROOTS,
    SOURCE_BUNDLE_MANIFESTS,
)


def frontend_path(path):
    parts = PurePosixPath(path).parts
    name = PurePosixPath(path).name
    return (parts[:3] == ("apps", "zoen", "src")
            and PurePosixPath(path).suffix in (".ts", ".tsx")
            and not any("generated" in part.lower() or part == "vendor" for part in parts)
            and not name.endswith(".d.ts")
            and not name.startswith("native-")
            and not any(marker in name for marker in (".native.", ".ios.", ".android."))
            and path not in ("apps/zoen/src/core/types.ts", "apps/zoen/src/core/driver.ts"))


def json_strings(value):
    if isinstance(value, dict):
        for key, child in value.items():
            yield key
            yield from json_strings(child)
    elif isinstance(value, list):
        for child in value:
            yield from json_strings(child)
    elif isinstance(value, str):
        yield value


def regular_blob(tree, path):
    entries = git("ls-tree", "--full-tree", "-z", tree, "--", path).split(b"\0")
    metadata, separator, name = entries[0].partition(b"\t")
    return (len(entries) == 2 and not entries[1] and separator == b"\t"
            and name == path.encode() and metadata.split()[:2] == [b"100644", b"blob"])


def verified_disjoint_readers(base, head, paths):
    admitted = {}
    for (reader, disjoint_path), (expected, consumed_path) in DISJOINT_FRONTEND_REFERENCES.items():
        if disjoint_path not in paths:
            continue
        for tree in (base, head):
            # A child-layout symlink could alias the changed root. Neither
            # missing files nor aliases are the reviewed literal reference.
            if (not regular_blob(tree, reader) or not regular_blob(tree, consumed_path)
                    or hashlib.sha256(git("show", tree + ":" + reader)).hexdigest() != expected):
                raise ValueError("Frontend disjoint reader contract drift: " + reader)
        admitted.setdefault(reader, set()).add(disjoint_path)
    return admitted


def frontend_reference_readers(tree, paths, reader=None):
    if not paths:
        return set()
    command = ["git", "grep", "-I", "-l", "-z", "-E"]
    for path in paths:
        for token in (path, PurePosixPath(path).name):
            command += ["-e", r"(^|[^A-Za-z0-9_.-])" + re.escape(token) + r"([^A-Za-z0-9_.-]|$)"]
    locations = [reader] if reader else [".", ":(exclude)apps/zoen/src/**",
        ":(exclude)*.md", ":(exclude)" + SELF, ":(exclude)" + TEST,
        ":(exclude)scripts/tests/test_ci_frontend_scope.py", ":(exclude)" + DISJOINT_TEST]
    search = subprocess.run(command + [tree, "--", *locations],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if search.returncode not in (0, 1):
        raise ValueError("Cannot establish the cross-boundary input closure")
    return {row.decode("utf-8").partition(":")[2]
            for row in search.stdout.split(b"\0") if row}


def validate_frontend(base, head, ancestor, paths):
    if not FRONTEND_CONTRACTS or not (set(RESOURCE_MANIFESTS) | NON_INPUT_APP_LITERALS).issubset(FRONTEND_CONTRACTS):
        raise ValueError("Incomplete frontend boundary contract")
    if ancestor != base:
        raise ValueError("Stale or non-ancestral base requires full Rust")
    if len(paths) > 128 or not all(frontend_path(path) for path in paths):
        raise ValueError("Not exclusively reviewed app TypeScript source")
    disjoint = verified_disjoint_readers(base, head, paths)
    names = {PurePosixPath(path).name for path in paths}
    for tree in (base, head):
        for path, expected in FRONTEND_CONTRACTS.items():
            if hashlib.sha256(git("show", tree + ":" + path)).hexdigest() != expected:
                raise ValueError("Frontend boundary contract drift: " + path)
        for path in paths:
            text = git("show", tree + ":" + path) if tree == head else None
            if text is not None and (b"\0" in text or not text.decode("utf-8")):
                raise ValueError("Binary or empty frontend input requires full Rust")
        listed = git("ls-tree", "-r", "--name-only", "-z", tree, "--",
                     "apps/zoen/src/miniapps/vendor", "crates/miniapps/resources", "miniapps").split(b"\0")
        manifests = {path.decode("utf-8") for path in listed if path and (
            path.endswith(b".manifest.json") or (path.startswith(b"miniapps/") and path.endswith(b"/manifest.json")))}
        if manifests != set(RESOURCE_MANIFESTS):
            raise ValueError("Unknown or missing embedded-resource manifest")
        for path in RESOURCE_MANIFESTS:
            data = json.loads(git("show", tree + ":" + path))
            if not isinstance(data, dict):
                raise ValueError("Invalid embedded-resource manifest: " + path)
            if path in SOURCE_BUNDLE_MANIFESTS:
                sources = data.get("sources")
                if (not isinstance(sources, dict) or not sources or len(sources) > 4096
                        or any(not isinstance(name, str) or not isinstance(digest, str)
                               or not re.fullmatch(r"[a-f0-9]{64}", digest)
                               for name, digest in sources.items())):
                    raise ValueError("Invalid bundled source closure: " + path)
            # Recursively include provenance/source maps too, not only direct
            # Rust include_str! inputs. A basename match is conservative.
            if any(value in paths or PurePosixPath(value).name in names
                   for value in json_strings(data)):
                raise ValueError("Frontend source is an embedded-resource input: " + path)
        # Cross-boundary readers and shared JSON can be outside the Cargo
        # directories. Frontend-to-frontend imports remain in the app lane.
        for reader in frontend_reference_readers(tree, paths):
            if reader not in disjoint or frontend_reference_readers(
                    tree, [path for path in paths if path not in disjoint[reader]], reader):
                raise ValueError("A tracked cross-boundary input refers to frontend source")
        command = ["git", "grep", "-I", "-l", "-z", "-F",
                   "-e", "apps/zoen/src", "-e", '"apps/zoen"',
                   "-e", "'apps/zoen'", "-e", '"apps"', "-e", "'apps'"]
        search = subprocess.run(command + [tree, "--", *RUST_ROOTS, ":(exclude)*.md"],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if search.returncode == 0:
            readers = {row.decode("utf-8").partition(":")[2]
                       for row in search.stdout.split(b"\0") if row}
            # These exact pinned manifests were parsed above, including every
            # declared source. They are known closures, not dynamic Rust readers.
            if readers - NON_INPUT_APP_LITERALS - set(RESOURCE_MANIFESTS):
                raise ValueError("A Rust input refers to frontend source or a broad app reader")
        elif search.returncode != 1:
            raise ValueError("Cannot establish the Rust/frontend input boundary")


def git(*args):
    return subprocess.run(["git", *args], check=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE).stdout


def classify(base, head, event):
    result = {"documentation_only": False, "frontend_only": False, "run_rust": True,
              "reason": "full-suite default", "paths": []}
    if event != "pull_request":
        result["reason"] = "Only pull requests may use the documentation lane"
        return result
    try:
        base = git("rev-parse", "--verify", base + "^{commit}").decode().strip()
        head = git("rev-parse", "--verify", head + "^{commit}").decode().strip()
        ancestors = git("merge-base", "--all", base, head).decode().splitlines()
        if len(ancestors) != 1:
            raise ValueError("Ambiguous merge bases require the full suite")
        ancestor = ancestors[0]
        entries = git("diff", "--raw", "--no-abbrev", "--no-renames", "-z",
                      ancestor, head, "--").split(b"\0")
        if entries[-1] != b"" or (len(entries) - 1) % 2:
            raise ValueError("Malformed Git diff requires the full suite")
        changes = []
        for index in range(0, len(entries) - 1, 2):
            metadata = entries[index].split()
            path = os.fsdecode(entries[index + 1])
            if len(metadata) != 5 or metadata[4] not in (b"A", b"M"):
                raise ValueError("Deleted, renamed or unusual changes require the full suite")
            if (metadata[1] != b"100644" or
                    (metadata[4] == b"M" and metadata[0] != b":100644")):
                raise ValueError("Unsupported tracked file mode: " + path)
            changes.append(path)
        result["paths"] = changes
        if not changes:
            raise ValueError("Empty or unavailable diff requires the full suite")
        if not all(path in DOCUMENTS for path in changes):
            validate_frontend(base, head, ancestor, changes)
            result.update(frontend_only=True, run_rust=False,
                          reason="Verified frontend-only PR; client/Expo and local integrity remain selected")
            return result
        # Both endpoints matter: removal of a consumer on this branch must not
        # conceal a dependency. Search all other non-Markdown tracked text,
        # including build scripts/configuration with unconventional extensions.
        for tree in (base, head):
            command = ["git", "grep", "-I", "-l", "-F"]
            for path in changes:
                command += ["-e", path, "-e", PurePosixPath(path).name]
            command += [tree, "--", ".", ":(exclude)*.md",
                        ":(exclude)" + SELF, ":(exclude)" + TEST]
            search = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            if search.returncode == 0:
                raise ValueError("A non-Markdown input refers to the changed prose")
            if search.returncode != 1:
                raise ValueError("Cannot establish the documentation dependency boundary")
        result.update(documentation_only=True, run_rust=False,
                      reason="Only allowlisted prose; no tracked consumer references")
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        result["reason"] = str(error)
    return result


def validate_documents(head, paths):
    """Validate changed prose only; historical oversized files are not waived."""
    for path in paths:
        blob = git("show", head + ":" + path)
        if len(blob) > 20_000:
            raise ValueError(path + " exceeds 5,000 tokens (bytes / 4). Split by topic into sibling documents and update links.")
        text = blob.decode("utf-8")
        # These allowlisted documents use ordinary inline links. External links
        # are not fetched; local targets must remain inside the tracked tree.
        for raw in re.findall(r"\]\((<[^>]+>|[^\s)]+)(?:\s+[^)]*)?\)", text):
            link = urlsplit(raw.strip("<>"))
            if link.scheme or link.netloc or not link.path:
                continue
            target = unquote(link.path)
            if target.startswith("/"):
                raise ValueError(path + ": local links must be repository-relative: " + target)
            parts = list(PurePosixPath(path).parent.parts)
            for part in PurePosixPath(target).parts:
                if part == "..":
                    if not parts:
                        raise ValueError(path + ": link escapes the repository")
                    parts.pop()
                elif part != ".":
                    parts.append(part)
            git("cat-file", "-e", head + ":" + "/".join(parts))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True)
    parser.add_argument("--head", required=True)
    parser.add_argument("--event", required=True)
    parser.add_argument("--check-documents", action="store_true")
    args = parser.parse_args()
    result = classify(args.base, args.head, args.event)
    if args.check_documents:
        if not result["documentation_only"]:
            raise ValueError("Documentation lane is not applicable: " + result["reason"])
        validate_documents(args.head, result["paths"])
        result["validation"] = "UTF-8, 20,000-byte limit and local link targets passed; Rust and integration tests were not executed"
    print(json.dumps(result, indent=2))
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            for key in ("documentation_only", "frontend_only", "run_rust"):
                output.write(key + "=" + str(result[key]).lower() + "\n")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
            summary.write("### CI scope\n\n" + result["reason"] + "\n\n")
            summary.write(result.get("validation", "Full Rust suite remains selected." if result["run_rust"]
                else "Rust job SKIPPED: no application Rust compilation or journeys. Local tooling fixtures and other applicable checks remain selected.") + "\n")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, UnicodeError, subprocess.SubprocessError) as error:
        print("Documentation integrity failed: " + str(error), file=sys.stderr)
        sys.exit(1)
