#!/usr/bin/env python3
"""Read and edit the one maintained acceptance document and its domain parts."""
import argparse
import json
import math
import os
from pathlib import Path, PurePosixPath
import stat
import tempfile


DEFAULT_MANIFEST = Path(__file__).resolve().parents[1] / "apps/zoen/acceptance.json"


class AcceptanceError(ValueError):
    pass


def _pairs(items):
    result = {}
    for key, value in items:
        if key in result:
            raise AcceptanceError(f"Duplicate JSON key: {key}")
        result[key] = value
    return result


def _constant(value):
    raise AcceptanceError(f"Invalid JSON constant: {value}")


def _float(value):
    result = float(value)
    if not math.isfinite(result):
        raise AcceptanceError("JSON float exceeds the supported finite range")
    return result


def read_json(path):
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"),
                          object_pairs_hook=_pairs, parse_constant=_constant, parse_float=_float)
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise AcceptanceError(f"Cannot read acceptance JSON: {path}: {error}") from error


def _part_path(parts, name):
    if not isinstance(name, str) or not name or "\\" in name:
        raise AcceptanceError("Part must be a relative POSIX JSON path")
    path = PurePosixPath(name)
    if path.is_absolute() or str(path) != name or ".." in path.parts or path.suffix != ".json":
        raise AcceptanceError(f"Invalid part path: {name}")
    target = parts.joinpath(*path.parts)
    if parts.is_symlink() or parts.parent.is_symlink():
        raise AcceptanceError("Part directory cannot use symlinks")
    current = parts
    for component in path.parts:
        current = current / component
        if current.is_symlink():
            raise AcceptanceError(f"Part cannot use symlinks: {name}")
    try:
        target.resolve().relative_to(parts.resolve())
    except ValueError as error:
        raise AcceptanceError(f"Part escapes canonical directory: {name}") from error
    return target


def _load(manifest, replacement=None):
    manifest = Path(manifest).resolve()
    parts = manifest.parent / "acceptance/parts"
    seen, active = set(), set()

    def expand(value, depth=0):
        if depth > 64:
            raise AcceptanceError("Acceptance nesting exceeds 64 levels")
        if isinstance(value, list):
            return [expand(item, depth + 1) for item in value]
        if not isinstance(value, dict):
            return value
        if "$part" not in value:
            return {key: expand(item, depth + 1) for key, item in value.items()}
        if list(value) != ["$part"]:
            raise AcceptanceError("A part reference must contain only $part")
        name = value["$part"]
        target = _part_path(parts, name)
        if target in active:
            raise AcceptanceError(f"Cyclic part: {name}")
        if target in seen:
            raise AcceptanceError(f"Duplicate part: {name}")
        seen.add(target)
        active.add(target)
        content = replacement[1] if replacement and target == replacement[0] else read_json(target)
        result = expand(content, depth + 1)
        active.remove(target)
        return result

    document = expand(read_json(manifest))
    if not isinstance(document, dict):
        raise AcceptanceError("Acceptance document must be an object")
    unused = set(parts.rglob("*.json")) - seen
    if unused:
        raise AcceptanceError(f"Unreferenced part: {sorted(unused)[0].relative_to(parts)}")
    return document, seen


def load_acceptance(manifest=DEFAULT_MANIFEST):
    """Return the fully composed logical record; never materialize another authority."""
    return _load(manifest)[0]


def write_part(name, value, manifest=DEFAULT_MANIFEST):
    """Validate the entire proposed graph, then atomically replace one existing part.

    This edits repository source, not product data. It makes no concurrent-writer
    CAS or multi-file transaction claim. Graph restructuring is a normal source
    edit followed by load/validation, rather than a second document store.
    """
    manifest = Path(manifest).resolve()
    _, current = _load(manifest)
    target = _part_path(manifest.parent / "acceptance/parts", name)
    if target not in current:
        raise AcceptanceError(f"Not a referenced canonical part: {name}")
    encoded = json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n"
    if len(encoded.encode("utf-8")) > 20000:
        raise AcceptanceError("A maintained part must not exceed 20000 bytes")
    _load(manifest, (target, value))
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=target.parent,
                                         prefix="." + target.name + ".", delete=False) as stream:
            temporary = Path(stream.name)
            stream.write(encoded)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary, stat.S_IMODE(target.stat().st_mode))
        os.replace(temporary, target)
    finally:
        if temporary and temporary.exists():
            temporary.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("read", help="Print composed JSON to stdout")
    commands.add_parser("check", help="Validate the canonical graph without writing")
    update = commands.add_parser("update-part", help="Validate and replace one referenced source part")
    update.add_argument("part")
    update.add_argument("--from-json", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "update-part":
            write_part(args.part, read_json(args.from_json), args.manifest)
        else:
            document = load_acceptance(args.manifest)
            if args.command == "read":
                print(json.dumps(document, ensure_ascii=False, indent=2, allow_nan=False))
    except (AcceptanceError, ValueError) as error:
        parser.exit(1, f"acceptance: {error}\n")


if __name__ == "__main__":
    main()
