#!/usr/bin/env python3
"""Install the versioned pre-commit hook without replacing any existing hook."""

import os
from pathlib import Path
import subprocess
import sys


def main():
    root = Path(__file__).resolve().parent.parent
    source = root / ".githooks" / "pre-commit"
    destination = Path(subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "--git-path", "hooks/pre-commit"],
        text=True,
    ).strip())
    if not destination.is_absolute():
        destination = root / destination
    contents = source.read_bytes()
    if destination.is_symlink():
        raise RuntimeError("Existing hook is a symlink; integrate manually: " + str(destination))
    if destination.exists():
        if destination.read_bytes() != contents:
            raise RuntimeError("Existing hook preserved; integrate manually: " + str(destination))
        destination.chmod(destination.stat().st_mode | 0o111)
    else:
        destination.parent.mkdir(parents=True, exist_ok=True)
        # Exclusive creation preserves a hook installed concurrently by another process.
        descriptor = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o755)
        with os.fdopen(descriptor, "wb") as output:
            output.write(contents)
        destination.chmod(destination.stat().st_mode | 0o111)
    print("Installed staged-file size hook: " + str(destination))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print("Hook installation failed: " + str(error), file=sys.stderr)
        sys.exit(1)
