#!/usr/bin/env python3
"""Guard direct local QA commands against the installed driver's text-entry bug."""

import re
import subprocess
import sys


BLOCKED = {"fill", "type"}
USAGE = "Usage: python3 scripts/qa-device.py <verb> [arguments/options]"


def main(argv=None):
    arguments = list(sys.argv[1:] if argv is None else argv)
    if not arguments or not re.fullmatch(r"[a-z][a-z0-9-]*", arguments[0]):
        print(USAGE, file=sys.stderr)
        print("Put the command verb first; leading options are refused before dispatch.",
              file=sys.stderr)
        return 2
    if arguments[0] in BLOCKED:
        print(
            "QA command refused before dispatch: fill and type are disabled. "
            "The installed iOS driver's clear/repair path can tap Send while "
            "positioning the caret (issue #526; CORE687).\n"
            "Use normal, verified selection/editing within an explicitly authorized "
            "scope, or leave the new draft intact. Do not fall back to the raw driver, "
            "retry through another interface, or infer that text was cleared.\n"
            "See docs/dev/iphone-qa-driver.md.",
            file=sys.stderr,
        )
        return 2
    try:
        # Inherit cwd, environment and standard streams. Keep arguments literal;
        # neither selectors nor text are interpreted by a shell.
        return subprocess.run(["agent-device", *arguments], check=False).returncode
    except FileNotFoundError:
        print("agent-device was not found on PATH; no command was dispatched.",
              file=sys.stderr)
        return 127
    except OSError:
        print("Could not start agent-device; no automatic retry was attempted.",
              file=sys.stderr)
        return 126


if __name__ == "__main__":
    sys.exit(main())
