#!/usr/bin/env python3
"""Every Material color slot the Android app reads must be set from the palette."""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
THEME = ROOT / "android/app/src/main/java/xyz/tironi/zoen/theme/Theme.kt"
EXTENSIONS = {"ownMessage", "onOwnMessage", "otherMessage"}


def read_slots():
    completed = subprocess.run(
        [
            "rg",
            "-o",
            r"MaterialTheme\.colorScheme\.[a-zA-Z]+",
            "android/app/src/main/java",
        ],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode not in (0, 1):
        print(completed.stderr)
        return None
    names = set()
    for line in completed.stdout.splitlines():
        names.add(line.rsplit(".", 1)[-1])
    return names


def call_body(text, kind):
    start = text.index(kind + "(")
    start = text.index("(", start)
    depth = 0
    for index, char in enumerate(text[start:], start):
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                return text[start + 1 : index]
    raise SystemExit("unclosed " + kind)


def top_level_names(body):
    names = []
    depth = 0
    token = ""
    for char in body:
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
        if depth == 0 and char == "=" and token.strip():
            names.append(token.strip().split()[-1])
            token = ""
            continue
        if depth == 0 and char == ",":
            token = ""
            continue
        token += char
    return set(names)


def builder_slots():
    text = THEME.read_text()
    found = {}
    for kind in ("lightColorScheme", "darkColorScheme"):
        if kind + "(" not in text:
            print("missing " + kind)
            return None
        found[kind] = top_level_names(call_body(text, kind))
    return found


def main():
    read = read_slots()
    built = builder_slots()
    if read is None or built is None:
        return 1
    light = built["lightColorScheme"]
    dark = built["darkColorScheme"]
    print("read")
    for name in sorted(read):
        print(name)
    print("builder")
    for name in sorted(light | dark):
        print(name)
    failed = False
    if light != dark:
        print("light and dark builders differ")
        print("only light " + " ".join(sorted(light - dark)))
        print("only dark " + " ".join(sorted(dark - light)))
        failed = True
    missing = sorted(read - light - EXTENSIONS)
    extra_extensions = sorted(read & EXTENSIONS)
    print("extensions")
    for name in extra_extensions:
        print(name)
    print("missing")
    for name in missing:
        print(name)
    if missing:
        failed = True
    if failed:
        print("slot check failed")
        return 1
    print("slot check passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
