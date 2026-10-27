"""Distinguish proven static evidence and exact other paths from app inputs."""
from pathlib import PurePosixPath
import re

from ci_static_documents import STATIC, validate


def material_readers(git, regular_blob, base, head, tree, paths, readers):
    metadata = set(readers) & STATIC
    if metadata:
        # Reuse the existing evidence consumer proof at BOTH endpoints. A new
        # runtime/build consumer or changed tooling reader invalidates it.
        validate(git, base, head, sorted(metadata))
    return {reader for reader in set(readers) - metadata
            if material_reference(git, regular_blob, base, head, tree, paths, reader)}


def material_reference(git, regular_blob, base, head, tree, paths, reader):
    text = git("show", tree + ":" + reader).decode("utf-8")
    tokens = set(paths) | {PurePosixPath(path).name for path in paths}
    references = re.compile(r"(^|[^A-Za-z0-9_.-])(" + "|".join(
        re.escape(token) for token in sorted(tokens, key=len, reverse=True)) + r")(?=$|[^A-Za-z0-9_.-])")
    # Only complete, unescaped quoted paths qualify. Bare basenames, relative
    # paths, templates and constructed/unknown targets remain material readers.
    literals = list(re.finditer(r'''(["'])([^"'\r\n\\]*)\1''', text))
    for match in references.finditer(text):
        disjoint = False
        for literal in literals:
            target = literal.group(2)
            if not (literal.start(2) <= match.start(2) and match.end(2) <= literal.end(2)):
                continue
            tail = text[literal.end():].lstrip()
            prefix = text[:literal.start()].rstrip()
            if (target.startswith("apps/zoen/src/") and str(PurePosixPath(target)) == target
                    and ".." not in PurePosixPath(target).parts and target not in paths
                    and PurePosixPath(target).suffix in (".ts", ".tsx")
                    and not tail.startswith((".", "[", "+", "%"))
                    and not prefix.endswith(("+", "%"))
                    and all(regular_blob(endpoint, target) for endpoint in (base, head))):
                disjoint = True
            break
        if not disjoint:
            return True
    return False
