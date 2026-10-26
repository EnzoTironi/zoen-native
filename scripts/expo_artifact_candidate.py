"""Bind scope decisions to the immutable checkout used by the artifact jobs."""
import re


def tested_candidate(git, base, head, event, candidate):
    if event not in ("pull_request", "push"):
        raise ValueError("Unknown event requires all Expo jobs")
    if not all(isinstance(value, str) and re.fullmatch(r"[0-9a-f]{40}", value)
               for value in (base, head, candidate)):
        raise ValueError("Missing or non-immutable artifact candidate")
    for value in (base, head, candidate):
        if git("rev-parse", "--verify", value + "^{commit}").decode().strip() != value:
            raise ValueError("Artifact candidate does not name an exact commit")
    if git("rev-parse", "HEAD").decode().strip() != candidate:
        raise ValueError("Artifact candidate differs from the tested checkout")
    if event == "push":
        if candidate != head:
            raise ValueError("Push candidate differs from its event head")
        return candidate
    parents = git("rev-list", "--parents", "-n", "1", candidate).decode().split()
    if parents != [candidate, base, head]:
        raise ValueError("PR candidate must have the exact event base/head parents")
    if len(git("merge-base", "--all", base, head).decode().splitlines()) != 1:
        raise ValueError("Ambiguous PR merge bases require all Expo jobs")
    # Parent IDs alone cannot authenticate a hand-written commit tree. A clean
    # canonical merge must reproduce the actual checkout; conflicts stay full.
    expected = git("merge-tree", "--write-tree", base, head).decode().strip()
    actual = git("rev-parse", candidate + "^{tree}").decode().strip()
    if not re.fullmatch(r"[0-9a-f]{40}", expected) or expected != actual:
        raise ValueError("PR candidate tree differs from the canonical event merge")
    return candidate
