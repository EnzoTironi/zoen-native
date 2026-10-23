#!/usr/bin/env python3
"""Select existing Expo jobs from immutable inputs, with a full-lane default."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tarfile
import tempfile

from expo_artifact_dependencies import compiler_inputs, graph


# These reviewed build contracts establish the roots in the metadata reader.
# Drift disables optimization, never a build or test. It does not require an ACK.
CONTRACTS = {
    ".github/workflows/expo.yml": "085e800264619bcc17f43b676a40b9ee682cb8e7dff845058b9ff4247e1bf7e7",
    "scripts/build-expo-core.mjs": "2f3d480e8b2ca4d57496aafe919f74a28371daf2ef3e92831258435d6422db47",
    "scripts/build-expo-web-core.mjs": "9504ad593e1413ed9b8e36c4b79032df1336e7ad740adbac18b5e9361fa8dfd2",
    "apps/zoen/package.json": "d58538007aec677d34171d4a09f883973c1c8bf9440b141d5bc72f33b2dba14b",
}
JOBS = ("android", "client", "release_web")


def git(*args):
    return subprocess.run(["git", *args], check=True, capture_output=True).stdout


def changes(base, head):
    entries = git("diff", "--raw", "--no-abbrev", "--no-renames", "-z", base, head, "--").split(b"\0")
    if entries[-1] != b"" or (len(entries) - 1) % 2:
        raise ValueError("Malformed Git diff")
    result = []
    for index in range(0, len(entries) - 1, 2):
        fields = entries[index].split()
        path = entries[index + 1].decode("utf-8", errors="strict")
        if (len(fields) != 5 or fields[4] not in (b"A", b"M")
                or fields[1] != b"100644"
                or (fields[4] == b"M" and fields[0] != b":100644")):
            raise ValueError("Deletion, rename, mode change or unusual diff")
        if (PurePosixPath(path).is_absolute() or ".." in PurePosixPath(path).parts
                or not path.endswith(".rs") or PurePosixPath(path).name == "build.rs"):
            raise ValueError("Cargo/config/tooling/unknown input requires all Expo jobs")
        result.append(path)
    if not result:
        raise ValueError("Empty diff requires all Expo jobs")
    return result


def manifests(tree):
    rows = git("ls-tree", "-r", "--full-tree", "-z", tree).split(b"\0")
    result = {}
    for row in rows:
        if not row:
            continue
        metadata, raw_path = row.split(b"\t", 1)
        path = raw_path.decode("utf-8", errors="strict")
        if (PurePosixPath(path).name in ("Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain", "rust-toolchain.toml")
                or ".cargo" in PurePosixPath(path).parts):
            result[path] = metadata
    return result


def snapshot(tree, directory):
    # A clean Git snapshot avoids current-worktree edits and shared Cargo targets.
    # Refuse links/special files rather than following an unmodelled input.
    archive = git("archive", "--format=tar", tree)
    with tarfile.open(fileobj=io.BytesIO(archive)) as source:
        for entry in source:
            path = PurePosixPath(entry.name)
            if path.is_absolute() or ".." in path.parts or not (entry.isfile() or entry.isdir()):
                raise ValueError("Unsupported archive input")
            destination = directory / entry.name
            if entry.isdir():
                destination.mkdir(parents=True, exist_ok=True)
            else:
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(source.extractfile(entry).read())


def foreign_consumer(tree, owners):
    # JS/tests can read Rust inputs without a Cargo edge. Catch literal foreign
    # paths and broad workspace readers in both endpoints. Unknown code/config
    # edits themselves never enter the optimized lane.
    roots = ["apps/zoen", "packages/zoen-ui", "packages/zoen-miniapp-sdk"]
    command = ["git", "grep", "-I", "-l", "-F", "-e", '"crates"', "-e", "'crates'"]
    for directory in owners:
        name = directory.name
        command += ["-e", name + "/", "-e", '"' + name + '"', "-e", "'" + name + "'"]
    result = subprocess.run(command + [tree, "--", *roots, ":(exclude)*.md"], capture_output=True)
    if result.returncode not in (0, 1):
        raise ValueError("Cannot establish tracked JS input boundary")
    return result.returncode == 0


def classify(base, head, event, cargo="cargo"):
    result = {"run_" + job: True for job in JOBS}
    result.update(reason="Full Expo lane default", paths=[], packages={})
    try:
        if event not in ("pull_request", "push"):
            raise ValueError("Unknown event requires all Expo jobs")
        base = git("rev-parse", "--verify", base + "^{commit}").decode().strip()
        head = git("rev-parse", "--verify", head + "^{commit}").decode().strip()
        ancestors = git("merge-base", "--all", base, head).decode().splitlines()
        if len(ancestors) != 1 or ancestors[0] != base:
            # A stale branch can omit new base consumers. Do not guess the
            # prospective merge's graph from its older manifests/source tree.
            raise ValueError("Ambiguous, stale-base or non-ancestral comparison")
        paths = changes(ancestors[0], head)
        result["paths"] = paths
        if manifests(base) != manifests(head):
            raise ValueError("Cargo/config inputs differ between endpoints")
        for tree in (base, head):
            for path, digest in CONTRACTS.items():
                if hashlib.sha256(git("show", tree + ":" + path)).hexdigest() != digest:
                    raise ValueError("Build contract drift: " + path)
        with tempfile.TemporaryDirectory(prefix="zoen-expo-scope-") as temporary:
            root = Path(temporary).resolve()
            snapshot(head, root)
            packages, closures = graph(root, cargo)
            owners = set()
            for path in paths:
                matches = [directory for directory in packages if directory in (root / path).parents]
                if len(matches) != 1:
                    raise ValueError("Changed Rust path has no unique Cargo owner")
                owners.update(matches)
            selected = {}
            for job, directories in closures.items():
                affected = bool(owners & directories)
                if not affected:
                    inputs = compiler_inputs(root, directories)
                    affected = any(root / path in inputs for path in paths)
                    affected |= any(foreign_consumer(tree, owners) for tree in (base, head))
                selected["run_" + job] = affected
                result["packages"][job] = sorted(packages[path]["name"] for path in directories)
            result.update(selected, reason="Verified Cargo dependency closures and pinned job/input contracts")
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError, tarfile.TarError) as error:
        result.update({"run_" + job: True for job in JOBS}, reason="Full Expo fallback: " + str(error))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True)
    parser.add_argument("--head", required=True)
    parser.add_argument("--event", required=True)
    parser.add_argument("--cargo", default="cargo")
    args = parser.parse_args()
    result = classify(args.base, args.head, args.event, args.cargo)
    print(json.dumps(result, indent=2))
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            for job in JOBS:
                output.write("run_" + job + "=" + str(result["run_" + job]).lower() + "\n")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
            summary.write("### Expo scope\n\n" + result["reason"] + "\n\n")
            for job in JOBS:
                summary.write("- " + job + ": " + ("selected" if result["run_" + job] else "SKIPPED; no matching artifact input") + "\n")
            summary.write("\nRust/relay CI is unchanged. SKIPPED jobs did not execute their builds/tests.\n")


if __name__ == "__main__":
    main()
