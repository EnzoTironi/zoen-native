#!/usr/bin/env python3
"""Inventory local, private living Space screenshots; never publishes media.

Expected captures are real application screens, not HTML mockups. This check only
proves that nonempty PNG files with the expected names exist, not UI parity or
behavior. Review each capture separately before making a parity claim.
"""
import argparse
import json
from pathlib import Path
import struct
import sys

PLATFORMS = ("iphone", "ipad", "android", "web-narrow", "web-wide")
SCREENS = ("home", "announcements", "page", "notifications", "hackathon", "teams", "submission", "results", "tiers")
THEMES = ("light", "dark")


def inventory(directory):
    entries = []
    for platform in PLATFORMS:
        for theme in THEMES:
            for screen in SCREENS:
                name = f"{platform}-{theme}-{screen}.png"
                path = directory / name
                entry = {"file": name, "status": "missing"}
                if path.is_file():
                    header = path.read_bytes()[:24]
                    if (len(header) == 24 and header[:8] == b"\x89PNG\r\n\x1a\n"
                            and header[12:16] == b"IHDR"):
                        width, height = struct.unpack(">II", header[16:24])
                        entry.update(status="present" if width and height else "invalid",
                                     width=width, height=height)
                    else:
                        entry["status"] = "invalid"
                entries.append(entry)
    return entries


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", nargs="?", type=Path,
                        default=Path.home() / "living-space-shots")
    args = parser.parse_args()
    entries = inventory(args.directory)
    print(json.dumps({"scope": "file inventory only; not behavioral or visual verification",
                      "captures": entries}, indent=2))
    return 0 if all(e["status"] == "present" for e in entries) else 1


if __name__ == "__main__":
    sys.exit(main())
