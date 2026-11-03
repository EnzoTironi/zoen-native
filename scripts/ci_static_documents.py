"""Reviewed static roadmap/evidence inputs shared by the two existing selectors."""
import hashlib
import json
import math
import re
from pathlib import PurePosixPath
import subprocess

ROOT = "docs/roadmap/"
STATIC = frozenset(ROOT + name for name in (
    "README.md", "index.html", "roadmap.css", "roadmap.js", "manifest.json",
    "visual-findings.md", "visual-review.json", "journeys/creation-capabilities.json",
    "journeys/crosscut-gates.json", "journeys/journeys-01-10.json",
    "journeys/journeys-11-20.json", "journeys/journeys-21-30.json",
    "surfaces/coverage.json", "surfaces/native-surfaces-01.json",
    "surfaces/presentations-01.json", "surfaces/routes-01.json",
    "surfaces/routes-02.json", "surfaces/system-ports-01.json",
)) | frozenset(ROOT + "telegram/" + name + ".json" for name in (
    "ai", "apps", "boosts", "bots", "business", "calls", "channels", "formatting",
    "groups", "inbox", "messaging", "notifications", "photos", "polls", "premium",
    "privacy", "profiles", "reactions", "search", "stars-economy", "stars-gifts",
    "storage", "stories", "voice",
)) | frozenset({
    "docs/dev/zoen-complete-product-execution.md", "docs/dev/zoen-complete-product-goal.md",
    "docs/dev/zoen-complete-product-ui-reference.md", "docs/dev/zoen-product-progress.md",
    "docs/dev/zoen-product-progress-apps-and-backend.md", "docs/dev/zoen-product-progress-native-v131.md",
})
MANIFEST = "apps/zoen/acceptance.json"
EXECUTION = "apps/zoen/acceptance/parts/scope/execution-plan.json"
ARTIFACT = "apps/zoen/acceptance/parts/artifacts/current-iphone-artifact.json"
POINTERS = frozenset({MANIFEST, EXECUTION, ARTIFACT})
# These exact tooling consumers compose/check evidence, not application inputs.
# The E2E writer's acceptance.json is an unrelated output file. Drift fails closed.
READERS = {
    # A source-only research register cites acceptance prose; no code consumes it.
    "docs/dev/signal-test-contracts.json": "381747f0c985d8d81153c5582dc15dd44bd3a30dfd1c2d6872db87d17dc6669c",
    "scripts/acceptance.py": "aa799dd3e3ea1b55fc4977abe61153620d0a030beb3662c4900f17ddac759724",
    "scripts/test-acceptance.py": "80871289a2b8583b6952fe5f39cd45ed6157f9b87e5c31283e6ff0349bd675ed",
    # This agent fixture writes visual-review.json under its private test root.
    "crates/zoen-cli/src/agent/scene_tool_tests/visual_review.rs": "cd5dd86aed8c53fd9844b6f62548866620a9a0bc3d1c1e3cfc6028ec9772da46",
    "scripts/expo-web-e2e.mjs": "69a98ebec7444d110eb581a0857fe2b626fbf9ae330f517391e88510314d5eb3",
}
CORPUS = "crates/zoen-cli/tests/journey_files.rs"
CORPUS_HASH = "28ac64c7f3550871e36f23664122dd885281711b82fd07c0c21d3862c6d09c83"
SELECTORS = {"scripts/ci_ui_scope.py", "scripts/tests/test_ci_ui_scope.py", "scripts/ci_static_documents.py", "scripts/ci-documentation-scope.py",
             "scripts/expo-artifact-scope.py", "scripts/tests/test_ci_static_scope.py",
             "scripts/tests/test_ci_documentation_scope.py",
             "scripts/tests/test_ci_frontend_readers_scope.py"}


def pairs(items):
    result = {}
    for key, value in items:
        if key in result:
            raise ValueError("Duplicate evidence JSON key")
        result[key] = value
    return result


def finite_float(value):
    parsed = float(value)
    if not math.isfinite(parsed):
        raise ValueError("Non-finite evidence JSON")
    return parsed


def decode(blob):
    return json.loads(blob, object_pairs_hook=pairs, parse_float=finite_float,
                      parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)))


def pointer_change(git, base, head, path):
    before, after = [decode(git("show", tree + ":" + path)) for tree in (base, head)]
    if not isinstance(before, dict) or not isinstance(after, dict):
        raise ValueError("Evidence pointer must remain an object")
    if path == MANIFEST:
        if any(not isinstance(document.get("currentAcceptanceSummary"), str)
               for document in (before, after)):
            raise ValueError("Acceptance summary must remain prose")
        keys = {"currentAcceptanceSummary"}
    elif path == EXECUTION:
        keys = {"publicationStatus", "roadmap"}
        # Editorial scope clarification only; milestone IDs/status/exit stay exact.
        for document in (before, after):
            milestones = document.get("milestones", [])
            if not isinstance(milestones, list) or any(not isinstance(item, dict) for item in milestones):
                raise ValueError("Execution milestones must remain objects")
            for milestone in milestones:
                if milestone.get("id") == "apps-and-durable-agents":
                    for key in ("activeScope", "pausedScope", "statusBoundary"):
                        value = milestone.pop(key, None)
                        if value is not None and not isinstance(value, str):
                            raise ValueError("Execution description must remain prose")
    else:
        keys = {"checkpoint", "commit", "frontendQualificationCommit", "integrationChecks",
                "archivePath", "executableSha256", "bundleSha256", "status", "buildVerification",
                "installedVerification", "runtimeAcceptance", "backend", "pending", "historicalEvidence",
                "restoreVerification", "historicalPointer", "qaBoundary"}
    for document in (before, after):
        for key in keys:
            document.pop(key, None)
    if json.dumps(before, sort_keys=True) != json.dumps(after, sort_keys=True):
        raise ValueError("Acceptance criteria, flags or unknown pointer fields changed")


def validate(git, base, head, paths, *, artifacts=False):
    """No Cargo/native inputs. Markdown corpus changes retain the Rust journey lane."""
    if not paths or not set(paths).issubset(STATIC | POINTERS):
        raise ValueError("Not exclusively reviewed static documentation/evidence")
    for path in paths:
        for tree in (base, head):
            # Adds need only exist at head; aliases and executable modes never qualify.
            entry = git("ls-tree", "--full-tree", "-z", tree, "--", path)
            if not entry and tree == base and path in STATIC:
                continue
            if not entry.startswith(b"100644 blob "):
                raise ValueError("Static evidence must be a regular non-executable blob")
            blob = git("show", tree + ":" + path)
            if b"\0" in blob or (tree == head and len(blob) > 20_000):
                raise ValueError("Invalid static evidence size or encoding")
            blob.decode("utf-8")
            if path.endswith(".json"):
                decode(blob)
        if path in POINTERS:
            pointer_change(git, base, head, path)
    for tree in (base, head):
        command = ["git", "grep", "-I", "-l", "-z", "-F"]
        # Common entry basenames name unrelated bundles. Their changed parent
        # directories (including split joins) remain conservative read tokens.
        generic = {"index.html", "README.md", "manifest.json", "coverage.json"}
        tokens = set(paths) | {PurePosixPath(path).name for path in paths
                              if PurePosixPath(path).name not in generic}
        tokens |= {"docs/roadmap", '"roadmap"', "'roadmap'", '"docs"', "'docs'", "apps/zoen/acceptance"}
        for token in sorted(tokens):
            command += ["-e", token]
        exclude = STATIC | SELECTORS
        command += [tree, "--", ".", ":(exclude)*.md", *[":(exclude)" + path for path in sorted(exclude)]]
        search = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if search.returncode not in (0, 1):
            raise ValueError("Cannot establish static documentation consumer boundary")
        candidates = {row.decode().partition(":")[2] for row in search.stdout.split(b"\0") if row}
        pattern = re.compile(r"(^|[^A-Za-z0-9_.-])(?:" + "|".join(re.escape(token) for token in tokens) + r")([^A-Za-z0-9_.-]|$)")
        readers = {reader for reader in candidates
                   if pattern.search(git("show", tree + ":" + reader).decode("utf-8"))}
        for reader in sorted(readers):
            if reader in POINTERS or (reader.startswith("apps/zoen/acceptance/parts/") and reader.endswith(".json")):
                continue  # Canonical evidence parts reference each other, not runtime code.
            expected = READERS.get(reader)
            if reader == CORPUS:
                if not artifacts and any(path.endswith(".md") for path in paths):
                    raise ValueError("Changed Markdown is a real Rust journey corpus input")
                expected = CORPUS_HASH
            if expected is None or hashlib.sha256(git("show", tree + ":" + reader)).hexdigest() != expected:
                raise ValueError("Unknown or changed static-document consumer: " + reader)
