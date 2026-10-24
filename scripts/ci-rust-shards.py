#!/usr/bin/env python3
"""Partition CLI test targets without omitting new integration journeys."""

import argparse
import json
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parent.parent
COUNT = 3


def commands(metadata, shard):
    members = set(metadata["workspace_members"])
    packages = [p for p in metadata["packages"]
                if p["name"] == "zoen-cli" and p["id"] in members]
    if len(packages) != 1 or shard not in range(COUNT):
        raise ValueError("Expected one workspace CLI package and shard 0, 1 or 2")
    targets = packages[0]["targets"]
    flags = {"lib": "--lib", "bin": "--bin", "test": "--test", "example": "--example"}
    selected = []
    for target in targets:
        kinds = target["kind"]
        if len(kinds) != 1 or kinds[0] not in flags:
            raise ValueError("Review new CLI target kind before partitioning: " + str(kinds))
        if target.get("required-features"):
            raise ValueError("Review feature-gated CLI test coverage: " + target["name"])
        if target["test"]:
            selected.append(target)
    selected.sort(key=lambda target: (target["name"], target["kind"]))
    if not selected[shard::COUNT]:
        raise ValueError("Expected nonempty CLI test shards")
    base = ["cargo", "test", "--locked", "--package", "zoen-cli", "--no-fail-fast"]
    run = base.copy()
    for target in selected[shard::COUNT]:
        kind = target["kind"][0]
        run.append(flags[kind])
        if kind != "lib":
            run.append(target["name"])
    plan = [run]
    if shard == 0:
        # Default cargo test also compiles examples without running them.
        if any(target["kind"] == ["example"] for target in targets):
            plan.append(base + ["--examples", "--no-run"])
        if any(target["kind"] == ["lib"] and target["doctest"] for target in targets):
            plan.append(base + ["--doc"])
    return plan


def execute(plan, cwd=ROOT):
    failed = False
    for command in plan:
        print(json.dumps({"command": command}), flush=True)
        failed = subprocess.run(command, cwd=cwd).returncode != 0 or failed
    return int(failed)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("shard", type=int, choices=range(COUNT))
    args = parser.parse_args()
    try:
        metadata = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
            cwd=ROOT, text=True,
        ))
        return execute(commands(metadata, args.shard))
    except (KeyError, ValueError, OSError, subprocess.SubprocessError) as error:
        print("Rust shard rejected: " + str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
