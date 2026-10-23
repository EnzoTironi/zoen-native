#!/usr/bin/env python3
"""Conservatively select the documentation-only PR lane from immutable Git trees."""

import argparse
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


def git(*args):
    return subprocess.run(["git", *args], check=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE).stdout


def classify(base, head, event):
    result = {"documentation_only": False, "reason": "full-suite default", "paths": []}
    if event != "pull_request":
        result["reason"] = "Only pull requests may use the documentation lane"
        return result
    try:
        base = git("rev-parse", "--verify", base + "^{commit}").decode().strip()
        head = git("rev-parse", "--verify", head + "^{commit}").decode().strip()
        ancestor = git("merge-base", base, head).decode().strip()
        entries = git("diff", "--raw", "--no-abbrev", "--no-renames", "-z",
                      ancestor, head, "--").split(b"\0")
        changes = []
        for index in range(0, len(entries) - 1, 2):
            metadata = entries[index].split()
            path = os.fsdecode(entries[index + 1])
            if len(metadata) != 5 or metadata[4] not in (b"A", b"M"):
                raise ValueError("Deleted, renamed or unusual changes require the full suite")
            if metadata[1] != b"100644" or path not in DOCUMENTS:
                raise ValueError("Changed path is outside the explicit prose allowlist: " + path)
            changes.append(path)
        result["paths"] = changes
        if not changes:
            raise ValueError("Empty or unavailable diff requires the full suite")
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
        result.update(documentation_only=True, reason="Only allowlisted prose; no tracked consumer references")
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
            output.write("documentation_only=" + str(result["documentation_only"]).lower() + "\n")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
            summary.write("### CI scope\n\n" + result["reason"] + "\n\n")
            summary.write(result.get("validation", "Full Rust and integration jobs remain selected." if not result["documentation_only"] else "Documentation integrity selected; Rust/integration jobs explicitly skipped.") + "\n")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, UnicodeError, subprocess.SubprocessError) as error:
        print("Documentation integrity failed: " + str(error), file=sys.stderr)
        sys.exit(1)
