"""Causal real-Git fixtures for metadata links and qualified route collisions."""
import json
from pathlib import Path

from ci_frontend_scope_fixture import FrontendRepositoryFixture

TARGET = "apps/zoen/src/app/agent/[id].tsx"
OTHER = "apps/zoen/src/app/chat/[id].tsx"
EVIDENCE = "docs/roadmap/telegram/bots.json"


class FrontendReaderScopeTests(FrontendRepositoryFixture):
    def prepare(self, *, metadata=False, body=None):
        self.write(TARGET, "export default 'before';\n")
        self.write(OTHER, "export default 'other';\n")
        if metadata:
            self.write(EVIDENCE, json.dumps([{"id": "bot", "evidence": [TARGET]}]))
        if body is not None:
            self.write("scripts/reader.py", body)
        self.base = self.commit()
        self.write(TARGET, "export default 'after';\n")

    def test_649_metadata_reference_only_skips_rust(self):
        self.prepare(metadata=True)
        self.assertFalse(self.classify()["run_rust"])

    def test_649_other_existing_qualified_route_only_skips_rust(self):
        self.prepare(body=f'route = "{OTHER}"\n')
        self.assertFalse(self.classify()["run_rust"])

    def test_649_both_false_positive_causes_skip_rust(self):
        self.prepare(metadata=True, body=f'route = "{OTHER}"\n')
        self.assertFalse(self.classify()["run_rust"])

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
