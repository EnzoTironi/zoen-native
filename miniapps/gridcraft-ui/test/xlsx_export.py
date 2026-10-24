#!/usr/bin/env python3
"""Inspect real exported Grid XLSX bytes; no UI, persistence or identity proof."""

import argparse
from decimal import Decimal, InvalidOperation
import json
import re
import sys
import zlib
from zipfile import BadZipFile

from xlsx_reader import cell_reference, inspect_file, require

HASH = re.compile(r"[0-9a-f]{64}\Z")


def read_json(path):
    with open(path, "rb") as source:
        raw = source.read(64 * 1024 + 1)
    require(len(raw) <= 64 * 1024, "JSON input byte bound")
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "duplicate JSON key")
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=unique)


def head(value):
    require(isinstance(value, dict) and set(value) == {"version", "sha256"},
            "sourceHead must contain version and sha256")
    version = value["version"]
    require(isinstance(version, str) and version.isascii() and version.isdecimal()
            and int(version) > 0 and str(int(version)) == version, "invalid source version")
    require(isinstance(value["sha256"], str) and HASH.fullmatch(value["sha256"]),
            "invalid source SHA256")
    return value


def verify(result, expected, metadata=None, context=None):
    require(isinstance(expected, dict) and 0 < len(expected) <= 40_000,
            "expected cells must be a nonempty object")
    for key, wanted in expected.items():
        cell_reference(key)
        require(isinstance(wanted, dict) and set(wanted) <= {"value", "formula"}
                and "value" in wanted, "expected numeric value required")
        actual = result["cells"].get(key)
        require(actual is not None, f"missing expected cell {key}")
        require(actual["type"] == "n", f"expected numeric cell {key}")
        require(isinstance(wanted["value"], str), "expected values must be decimal strings")
        try:
            left, right = Decimal(actual["value"]), Decimal(wanted["value"])
            require(left.is_finite() and right.is_finite() and left == right,
                    f"numeric value mismatch at {key}")
        except InvalidOperation as error:
            raise ValueError(f"invalid numeric value at {key}") from error
        if "formula" in wanted:
            require(isinstance(wanted["formula"], str)
                    and actual["formula"] == wanted["formula"], f"formula mismatch at {key}")
        else:
            require(actual["formula"] == "", f"unexpected formula at {key}")
    source = None
    if metadata is not None:
        require(isinstance(metadata, dict) and metadata.get("format") == "xlsx", "metadata format mismatch")
        source = head(metadata.get("sourceHead"))
        require(metadata.get("sha256") == result["sha256"], "export SHA256 mismatch")
        require(type(metadata.get("bytes")) is int and metadata["bytes"] == result["bytes"],
                "export byte count mismatch")
        if "byteLength" in metadata:
            require(type(metadata["byteLength"]) is int and metadata["byteLength"] == result["bytes"],
                    "export byteLength mismatch")
    if context is not None:
        require(isinstance(context, dict) and set(context) == {"itemId", "sourceHead"},
                "operator context must contain itemId and sourceHead")
        require(isinstance(context["itemId"], str) and 0 < len(context["itemId"]) <= 256
                and all(ord(c) >= 32 for c in context["itemId"]), "invalid context itemId")
        supplied = head(context["sourceHead"])
        require(source is not None, "operator context requires captured export metadata")
        require(supplied == source, "operator context and metadata head mismatch")
    return {"result": "PASS", "sha256": result["sha256"], "bytes": result["bytes"],
            "sheet": result["sheet"], "checkedCells": list(expected),
            "exportMetadataMatched": metadata is not None,
            "operatorContext": context, "identityBindingVerified": False,
            "uiJourneyVerified": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("cells", "verify"))
    parser.add_argument("file")
    parser.add_argument("--sheet", default="Sheet1")
    parser.add_argument("--expect", help="JSON mapping cell references to numeric value/formula strings")
    parser.add_argument("--metadata", help="Captured prepare_document_export metadata, not reconstructed")
    parser.add_argument("--context", help="Operator-supplied itemId/sourceHead; not cryptographic identity proof")
    args = parser.parse_args()
    result = inspect_file(args.file, args.sheet)
    if args.mode == "cells":
        require(not (args.expect or args.metadata or args.context), "cells takes no verification inputs")
        # Preserve the existing browser test reader's public shape.
        output = {key: {"value": cell["value"], "formula": cell["formula"]}
                  for key, cell in result["cells"].items()}
    else:
        require(args.expect is not None, "verify requires --expect")
        output = verify(result, read_json(args.expect),
                        read_json(args.metadata) if args.metadata else None,
                        read_json(args.context) if args.context else None)
    print(json.dumps(output, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, BadZipFile, KeyError, RuntimeError, zlib.error, NotImplementedError) as error:
        # No archive bytes, XML contents, metadata or private identity in errors.
        print(json.dumps({"result": "FAIL", "error": type(error).__name__}), file=sys.stderr)
        sys.exit(1)
