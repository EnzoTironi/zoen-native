"""Causal real-Git fixtures for metadata links and qualified route collisions."""
import json
from pathlib import Path

from ci_frontend_scope_fixture import FrontendRepositoryFixture, SOURCE

TARGET = "apps/zoen/src/app/agent/[id].tsx"
OTHER = "apps/zoen/src/app/chat/[id].tsx"
EVIDENCE = "docs/roadmap/telegram/bots.json"
SYNTHETIC = "scripts/test-android-core-contract.py"


class FrontendReaderScopeTests(FrontendRepositoryFixture):
    def prepare(self, *, metadata=False, body=None, synthetic=False):
        self.write(TARGET, "export default 'before';\n")
        self.write(OTHER, "export default 'other';\n")
        if metadata:
            self.write(EVIDENCE, json.dumps([{"id": "bot", "evidence": [TARGET]}]))
        if synthetic:
            self.write(SYNTHETIC, (SOURCE.parents[1] / SYNTHETIC).read_bytes())
        if body is not None:
            self.write("scripts/reader.py", body)
        self.base = self.commit()
        self.write(TARGET, "export default 'after';\n")

    def test_649_metadata_reference_only_skips_rust(self):
        self.prepare(metadata=True)
        self.assertFalse(self.classify()["run_rust"])

    def test_649_reviewed_synthetic_reader_only_skips_rust(self):
        self.prepare(synthetic=True)
        self.assertFalse(self.classify()["run_rust"])

    def test_649_both_false_positive_causes_skip_rust(self):
        self.prepare(metadata=True, synthetic=True)
        self.assertFalse(self.classify()["run_rust"])

    def test_synthetic_reader_drift_restores_full_lane(self):
        original = self.base
        body = (SOURCE.parents[1] / SYNTHETIC).read_text()
        for changed in (body + "\n# changed reader\n", body.replace(OTHER, TARGET)):
            self.git("reset", "--hard", original)
            self.git("clean", "-fdq")
            self.prepare(synthetic=True)
            self.write(SYNTHETIC, changed)
            self.base = self.commit()
            self.write(TARGET, "export default 'another';\n")
            result = self.classify()
            self.assert_full(result)
            self.assertIn("Synthetic frontend reader contract drift", result["reason"])

    def test_metadata_with_real_rust_include_consumer_keeps_rust(self):
        self.prepare(metadata=True)
        self.write("crates/fixture/src/lib.rs", f'const INPUT: &str = include_str!("../../../{EVIDENCE}");\n')
        self.base = self.commit()
        self.write(TARGET, "export default 'another';\n")
        result = self.classify()
        self.assert_full(result)
        self.assertIn("static-document consumer", result["reason"])

    def test_unknown_json_is_not_static_evidence(self):
        original = self.base
        for path in ("contracts/unknown.json", "docs/roadmap/telegram/unknown.json"):
            self.git("reset", "--hard", original)
            self.git("clean", "-fdq")
            self.prepare()
            self.write(path, json.dumps({"evidence": [TARGET]}))
            self.base = self.commit()
            self.write(TARGET, "export default 'another';\n")
            self.assert_full(self.classify())

    def test_bare_relative_dynamic_unknown_and_direct_targets_keep_rust(self):
        bodies = [
            f'route = "{OTHER}"\n',
            f'p = "{OTHER}"\np = p.replace("/chat/", "/agent/")\nread(p)\n',
            'route = "[id].tsx"\n',
            'route = "../app/agent/[id].tsx"\n',
            'route = f"apps/zoen/src/app/{kind}/[id].tsx"\n',
            'route = "apps/zoen/src/app/unknown/[id].tsx"\n',
            f'route = "{TARGET}"\n',
            f'route = "{OTHER}".replace("chat", "agent")\n',
            f'route = "{OTHER}" + suffix\n',
            f'route = prefix + "{OTHER}"\n',
            f'route = "{OTHER}"\nunknown = "[id].tsx"\n',
        ]
        original = self.base
        for body in bodies:
            with self.subTest(body=body):
                self.git("reset", "--hard", original)
                self.git("clean", "-fdq")
                self.prepare(body=body)
                self.assert_full(self.classify())

    def test_qualified_symlink_and_changed_declared_target_keep_rust(self):
        self.prepare(body=f'route = "{OTHER}"\n')
        Path(OTHER).unlink()
        Path(OTHER).symlink_to("../agent/[id].tsx")
        self.base = self.commit()
        self.write(TARGET, "export default 'another';\n")
        self.assert_full(self.classify())
        self.git("reset", "--hard", self.base)
        Path(OTHER).unlink()
        self.write(OTHER, "export default 'regular';\n")
        self.base = self.commit()
        self.write(TARGET, "export default 'changed';\n")
        self.write(OTHER, "export default 'also changed';\n")
        self.assert_full(self.classify())
