#!/usr/bin/env python3
"""CI equivalent of the staged-only hook, applied to the proposed commit delta."""
import argparse
import os
from pathlib import Path
import runpy
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True)
    parser.add_argument("--head", required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    os.chdir(root)
    policy = runpy.run_path(str(root / ".githooks/pre-commit"), run_name="size_policy")
    git = policy["git"]
    base = args.base
    if len(base) in (40, 64) and set(base) == {"0"}:
        # GitHub's initial push has no prior commit. Compare with the empty tree.
        base = git("hash-object", "-t", "tree", "-w", "--stdin", input_bytes=b"").decode().strip()
    base_tree = git("rev-parse", "--verify", "--end-of-options", base + "^{tree}").strip()
    head_tree = git("rev-parse", "--verify", "--end-of-options", args.head + "^{tree}").strip()
    candidates = policy["diff_candidates"]((base_tree.decode(), head_tree.decode()))
    return policy["check_candidates"](candidates)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print("Changed-file size check failed: " + str(error), file=sys.stderr)
        sys.exit(1)
