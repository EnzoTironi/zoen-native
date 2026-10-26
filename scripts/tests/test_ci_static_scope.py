"""Static roadmap/evidence boundaries over real immutable Git histories; no Cargo."""
import importlib.util
import json
from pathlib import Path
import unittest
from unittest.mock import patch

import test_ci_documentation_scope as fixtures
CI = fixtures.SCOPE

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("static_expo_scope", ROOT / "scripts/expo-artifact-scope.py")
EXPO = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(EXPO)
STATIC = EXPO.validate_static_documents.__globals__


class StaticScopeTests(unittest.TestCase):
    setUp = fixtures.DocumentationScopeTests.setUp
    git = fixtures.DocumentationScopeTests.git
    write = fixtures.DocumentationScopeTests.write
    commit = fixtures.DocumentationScopeTests.commit

    def full(self, head):
        result = self.classify(head)
        return all(result["run_" + job] for job in EXPO.JOBS)

    def evidence(self):
        self.write("apps/zoen/acceptance.json", json.dumps({"currentAcceptanceSummary": "before", "accepted": False, "requiredJourneys": ["one"]}))
        self.write("apps/zoen/acceptance/parts/scope/execution-plan.json", json.dumps({"publicationStatus": "before", "iosLaunchAccepted": False, "milestones": [{"id": "apps-and-durable-agents", "status": "IN_PROGRESS", "activeScope": "before"}]}))
        self.write("apps/zoen/acceptance/parts/artifacts/current-iphone-artifact.json", json.dumps({"checkpoint": "707", "physicalDeviceAccepted": False, "coreCommit": "a" * 40}))
        self.base = self.commit()

    def classify(self, head):
        tree = self.git("merge-tree", "--write-tree", self.base, head)
        candidate = self.git("commit-tree", tree, "-p", self.base, "-p", head, "-m", "canonical merge")
        self.git("checkout", "--detach", "-q", candidate)
        with patch.object(EXPO, "graph", side_effect=AssertionError("Cargo metadata must not run")):
            return EXPO.classify(self.base, head, "pull_request", "/must-not-run-cargo", candidate)

    def test_static_viewer_and_exact_pointer_fields_skip_artifacts_and_rust(self):
        corpus = "crates/zoen-cli/tests/journey_files.rs"
        self.write(corpus, (ROOT / corpus).read_text())
        self.write("scripts/acceptance.py", (ROOT / "scripts/acceptance.py").read_text())
        self.evidence()
        self.write("docs/roadmap/index.html", "<main>Roadmap</main>\n")
        self.write("docs/roadmap/roadmap.js", "fetch('manifest.json');\n")
        self.write("docs/roadmap/manifest.json", '{"sourceCommit":"' + self.base + '"}')
        self.write("apps/zoen/acceptance.json", json.dumps({"currentAcceptanceSummary": "new evidence", "accepted": False, "requiredJourneys": ["one"]}))
        self.write("apps/zoen/acceptance/parts/artifacts/current-iphone-artifact.json", json.dumps({"checkpoint": "718", "physicalDeviceAccepted": False, "coreCommit": "a" * 40}))
        execution = "apps/zoen/acceptance/parts/scope/execution-plan.json"
        document = json.loads(Path(execution).read_text())
        document["publicationStatus"] = "new immutable evidence"
        document["milestones"][0]["activeScope"] = "agents only"
        document["milestones"][0]["statusBoundary"] = "Apps remain paused"
        document["roadmap"] = {"guide": "docs/roadmap/README.md", "manifestSnapshot": "docs/roadmap/manifest.json", "policy": "derived"}
        self.write(execution, json.dumps(document))
        head = self.commit()
        result = CI.classify(self.base, head, "pull_request")
        self.assertTrue(result["documentation_only"], result)
        self.assertFalse(result["run_rust"], result)
        CI.validate_documents(head, result["paths"])
        result = self.classify(head)
        self.assertFalse(any(result["run_" + job] for job in EXPO.JOBS), result)
        self.assertFalse(result["run_android_js"], result)

    def test_real_markdown_corpus_remains_rust_input_but_not_an_expo_input(self):
        corpus = "crates/zoen-cli/tests/journey_files.rs"
        self.write(corpus, (ROOT / corpus).read_text())
        self.base = self.commit()
        self.write("docs/roadmap/README.md", "# Roadmap\n")
        head = self.commit()
        result = CI.classify(self.base, head, "pull_request")
        self.assertTrue(result["run_rust"], result)
        self.assertFalse(result["documentation_only"], result)
        result = self.classify(head)
        self.assertFalse(any(result["run_" + job] for job in EXPO.JOBS), result)

    def test_existing_prose_lane_also_retains_its_real_directory_consumer(self):
        corpus = "crates/zoen-cli/tests/journey_files.rs"
        self.write(corpus, (ROOT / corpus).read_text())
        self.base = self.commit()
        self.write(fixtures.DOC, "Updated corpus member\n")
        result = CI.classify(self.base, self.commit(), "pull_request")
        self.assertTrue(result["run_rust"], result)
        self.assertFalse(result["documentation_only"], result)

    def test_unknown_files_modes_and_runtime_mix_keep_full_jobs(self):
        for path in ["docs/roadmap/arbitrary.js", "docs/roadmap/telegram/unknown.json", "apps/zoen/src/app/search.tsx", "apps/zoen/modules/zoen-core/ios/New.swift", "Cargo.toml", "crates/documents/src/lib.rs", ".github/workflows/ci.yml", ".githooks/pre-commit", "scripts/ci_static_documents.py"]:
            with self.subTest(path=path):
                self.git("reset", "--hard", self.base)
                self.git("clean", "-fdq")
                self.write("docs/roadmap/index.html", "<main>Static</main>")
                self.write(path, "unknown input")
                head = self.commit()
                self.assertTrue(CI.classify(self.base, head, "pull_request")["run_rust"])
                self.assertTrue(self.full(head))
        self.git("reset", "--hard", self.base)
        self.git("clean", "-fdq")
        self.write("docs/roadmap/index.html", "<main>Static</main>")
        Path("docs/roadmap/index.html").chmod(0o755)
        self.assertTrue(self.full(self.commit()))

    def test_acceptance_flags_scope_parts_and_unknown_fields_do_not_qualify(self):
        self.evidence()
        for key, value in [("accepted", True), ("accepted", 0), ("requiredJourneys", []), ("newRuntimeConfiguration", "enabled")]:
            with self.subTest(key=key):
                self.git("reset", "--hard", self.base)
                self.write("apps/zoen/acceptance.json", json.dumps({"currentAcceptanceSummary": "after", "accepted": False, "requiredJourneys": ["one"], key: value}))
                result = self.classify(self.commit())
                self.assertTrue(all(result["run_" + job] for job in EXPO.JOBS), result)

    def test_runtime_consumer_or_changed_known_reader_fails_closed_at_both_endpoints(self):
        for source in ['include_str!("../../../docs/roadmap/index.html");', 'read(join("docs", "roadmap", "index.html"));']:
            with self.subTest(source=source):
                self.git("reset", "--hard", self.base)
                self.write("crates/documents/src/lib.rs", source)
                base = self.commit()
                self.base = base
                self.write("docs/roadmap/index.html", "<main>changed</main>")
                head = self.commit()
                self.assertTrue(self.full(head))
                # Removing the consumer cannot conceal its old endpoint.
                self.write("crates/documents/src/lib.rs", "pub fn empty() {}")
                self.assertTrue(self.full(self.commit()))
        self.write("scripts/acceptance.py", 'read("docs/roadmap/index.html")')
        self.base = self.commit()
        self.write("docs/roadmap/index.html", "new")
        self.assertTrue(self.full(self.commit()))

    def test_duplicate_json_bad_encoding_and_noncanonical_candidate_do_not_skip(self):
        for body in [b'{"x":1,"x":2}', b'{"x":NaN}', b'{"x":1e999}', b'\xff', b'\0', b'x' * 20001]:
            with self.subTest(body=body[:20]):
                self.git("reset", "--hard", self.base)
                self.git("clean", "-fdq")
                path = Path("docs/roadmap/manifest.json")
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(body)
                self.assertTrue(self.full(self.commit()))
        self.git("reset", "--hard", self.base)
        self.git("clean", "-fdq")
        self.write("docs/roadmap/index.html", "static")
        head = self.commit()
        result = EXPO.classify(self.base, head, "pull_request", "/must-not-run-cargo", head)
        self.assertTrue(all(result["run_" + job] for job in EXPO.JOBS), result)


if __name__ == "__main__":
    unittest.main()
