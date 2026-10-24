"""Bounded inspection of exported managed Grid XLSX, never archive extraction.

This is a QA file reader, not the production XLSX admission/profile validator.
It preserves the browser fixture's raw value/formula representation.
"""

from hashlib import sha256
from io import BytesIO
from pathlib import PurePosixPath
import re
import stat
from xml.etree import ElementTree as ET
from zipfile import ZIP_DEFLATED, ZIP_STORED, ZipFile

MAX_ARCHIVE = 8 * 1024 * 1024
MAX_PART = 4 * 1024 * 1024
MAX_TOTAL = 16 * 1024 * 1024
MAX_ENTRIES = 128
MAX_CELLS = 40_000
MAIN = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
REL = "http://schemas.openxmlformats.org/package/2006/relationships"
DOC_REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
CELL = re.compile(r"([A-Z]{1,3})([1-9][0-9]{0,6})\Z")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def cell_reference(value):
    match = CELL.fullmatch(value)
    require(match is not None, "invalid cell reference")
    column = 0
    for letter in match[1]:
        column = column * 26 + ord(letter) - ord("A") + 1
    require(column <= 256 and int(match[2]) <= 10_000, "cell outside managed bounds")
    return value


def safe_name(name):
    require(isinstance(name, str) and name and "\\" not in name,
            "unsafe archive member")
    parts = name.split("/")
    require(not name.startswith("/") and all(p not in ("", ".", "..") for p in parts),
            "unsafe archive member")
    require(not any(ord(c) < 32 or c == ":" for c in name), "unsafe archive member")
    return name


def xml(raw):
    # The writer emits UTF-8. Reject UTF-16/32 and declarations before parsing;
    # ElementTree must never expand a supplied DTD/entity.
    require(b"\x00" not in raw, "unsupported XML encoding")
    upper = raw.upper()
    require(b"<!DOCTYPE" not in upper and b"<!ENTITY" not in upper,
            "XML DTD/entity forbidden")
    try:
        return ET.fromstring(raw)
    except ET.ParseError as error:
        raise ValueError("malformed XML") from error


def package(data):
    require(0 < len(data) <= MAX_ARCHIVE, "archive byte bound")
    with ZipFile(BytesIO(data)) as archive:
        entries = archive.infolist()
        require(0 < len(entries) <= MAX_ENTRIES, "archive entry bound")
        names, total = set(), 0
        # Complete central-directory preflight BEFORE opening/inflating any part.
        for entry in entries:
            name = safe_name(entry.filename)
            require(name not in names, "duplicate archive member")
            names.add(name)
            mode = entry.external_attr >> 16
            require(not stat.S_ISLNK(mode) and not entry.is_dir(), "non-file archive member")
            require(not entry.flag_bits & 1, "encrypted archive member")
            require(entry.compress_type in (ZIP_STORED, ZIP_DEFLATED), "unsupported compression")
            require(0 <= entry.file_size <= MAX_PART, "part byte bound")
            total += entry.file_size
            require(total <= MAX_TOTAL, "inflated total bound")
        parts = {}
        for entry in entries:
            with archive.open(entry) as source:
                raw = source.read(MAX_PART + 1)
                require(len(raw) == entry.file_size and len(raw) <= MAX_PART,
                        "actual part byte bound")
            # Reading through EOF validates CRC, also for non-selected parts.
            if entry.filename.endswith((".xml", ".rels")):
                xml(raw)
            parts[entry.filename] = raw
    return parts


def relationships(parts, path, base):
    require(path in parts, "missing workbook relationship part")
    root = xml(parts[path])
    require(root.tag == f"{{{REL}}}Relationships", "invalid relationship root")
    result = {}
    for node in root:
        require(node.tag == f"{{{REL}}}Relationship", "invalid relationship element")
        key, target = node.get("Id"), node.get("Target")
        require(key and key not in result and target, "duplicate/invalid relationship")
        require(node.get("TargetMode", "Internal") == "Internal", "external relationship")
        # Managed writer uses package-relative targets without traversal.
        safe_name(target)
        name = str(PurePosixPath(base) / target) if base else target
        require(name in parts, "missing relationship target")
        result[key] = (node.get("Type"), name)
    return result


def inspect_bytes(data, sheet="Sheet1"):
    parts = package(data)
    roots = relationships(parts, "_rels/.rels", "")
    documents = [name for kind, name in roots.values() if kind == DOC_REL + "/officeDocument"]
    require(documents == ["xl/workbook.xml"], "unsupported workbook location")
    root = xml(parts["xl/workbook.xml"])
    require(root.tag == f"{{{MAIN}}}workbook", "invalid workbook root")
    links = relationships(parts, "xl/_rels/workbook.xml.rels", "xl")
    sheets = root.findall(f"{{{MAIN}}}sheets/{{{MAIN}}}sheet")
    require(0 < len(sheets) <= 16, "sheet count bound")
    require(len({s.get("name") for s in sheets}) == len(sheets), "duplicate sheet name")
    chosen = [s for s in sheets if s.get("name") == sheet]
    require(len(chosen) == 1, "selected sheet missing")
    link = links.get(chosen[0].get(f"{{{DOC_REL}}}id"))
    require(link and link[0] == DOC_REL + "/worksheet", "invalid worksheet relationship")
    root = xml(parts[link[1]])
    require(root.tag == f"{{{MAIN}}}worksheet", "invalid worksheet root")
    cells = {}
    for node in root.findall(f"{{{MAIN}}}sheetData/{{{MAIN}}}row/{{{MAIN}}}c"):
        key = cell_reference(node.get("r", ""))
        require(key not in cells, "duplicate worksheet cell")
        require(len(cells) < MAX_CELLS, "stored cell bound")
        values, formulas = node.findall(f"{{{MAIN}}}v"), node.findall(f"{{{MAIN}}}f")
        require(len(values) <= 1 and len(formulas) <= 1, "ambiguous cell contents")
        # The raw v may be a shared-string index; callers must not mistake it for
        # resolved text. Numeric assertions below separately require numeric kind.
        cells[key] = {"value": values[0].text or "" if values else "",
                      "formula": formulas[0].text or "" if formulas else "",
                      "type": node.get("t", "n")}
    return {"sha256": sha256(data).hexdigest(), "bytes": len(data), "sheet": sheet,
            "cells": cells}


def inspect_file(path, sheet="Sheet1"):
    # Bounded read before ZipFile/central-directory allocation, including pipes or
    # a concurrently growing file. The digest describes these exact read bytes.
    with open(path, "rb") as source:
        data = source.read(MAX_ARCHIVE + 1)
    return inspect_bytes(data, sheet)
