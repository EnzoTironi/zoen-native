"""Exact disjoint-reader exception over real immutable Git snapshots."""
import hashlib
from pathlib import Path
import unittest

from ci_frontend_scope_fixture import SCOPE, SOURCE, FrontendRepositoryFixture

ROOT_LAYOUT = "apps/zoen/src/app/_layout.tsx"
CHILD_LAYOUT = "apps/zoen/src/app/miniapp/_layout.tsx"
DETAIL = "apps/zoen/src/miniapps/detail-screen.tsx"
REVIEW = "apps/zoen/src/miniapps/app-request-review.ts"
READER = "miniapps/excalidraw-ui/test/native-host-navigation-fixture.mjs"
READER_SHA = "177d00b51ff5669e9ccd4f16c0853890c9e2115472b97fd926ebf12b887fb902"


class DisjointLayoutScopeTests(FrontendRepositoryFixture):
    def setUp(self):
        super().setUp()
        self.reader = (SOURCE.parents[1] / READER).read_bytes()
        self.assertEqual(hashlib.sha256(self.reader).hexdigest(), READER_SHA)
        self.write(READER, self.reader)
        for path in (ROOT_LAYOUT, CHILD_LAYOUT, DETAIL, REVIEW):
            self.write(path, "export const screen = 'original';\n")
        self.base = self.commit()

    def edit(self, *paths):
        for path in paths:
            self.write(path, Path(path).read_bytes() + b"\n// Changed UI\n")

    def restore(self):
        self.git("reset", "--hard", self.base)
        self.git("clean", "-fdq")

    def test_pinned_other_layout_reference_is_disjoint(self):
        self.edit(ROOT_LAYOUT)
        result = self.classify()
        self.assertTrue(result["frontend_only"], result)
        self.assertFalse(result["run_rust"], result)
        self.assertEqual(result["paths"], [ROOT_LAYOUT])

    def test_actual_child_and_root_plus_child_keep_full(self):
        for paths in ((CHILD_LAYOUT,), (ROOT_LAYOUT, CHILD_LAYOUT)):
            with self.subTest(paths=paths):
                self.restore()
                self.edit(*paths)
                result = self.classify()
                self.assert_full(result)
                self.assertIn("cross-boundary", result["reason"])

    def test_another_consumed_path_cannot_hide_beside_root(self):
        self.edit(ROOT_LAYOUT, DETAIL)
        result = self.classify()
        self.assert_full(result)
        self.assertIn("cross-boundary", result["reason"])

    def test_real_review_parser_is_an_input_alone_and_beside_root(self):
        for paths in ((REVIEW,), (ROOT_LAYOUT, REVIEW)):
            with self.subTest(paths=paths):
                self.restore()
                self.edit(*paths)
                result = self.classify()
                self.assert_full(result)
                self.assertIn("cross-boundary", result["reason"])

    def test_reader_drift_is_full_even_when_only_root_changes(self):
        self.write(READER, self.reader + b"\n// Existing drift\n")
        self.base = self.commit()
        self.edit(ROOT_LAYOUT)
        result = self.classify()
        self.assert_full(result)
        self.assertIn("disjoint reader contract drift", result["reason"])

    def test_each_endpoint_pin_is_checked_independently(self):
        original = self.base
        for bad in ("base", "head"):
            with self.subTest(endpoint=bad):
                self.base = original
                self.restore()
                if bad == "base":
                    self.write(READER, self.reader + b"\n// Old drift\n")
                base = self.commit() if bad == "base" else original
                self.edit(ROOT_LAYOUT)
                self.write(READER, self.reader if bad == "base" else self.reader + b"\n// New drift\n")
                head = self.commit()
                # The classifier already rejects this mixed diff. The shared
                # proof port must independently refuse either stale pin too.
                self.assert_full(SCOPE.classify(base, head, "pull_request"))
                with self.assertRaisesRegex(ValueError, "disjoint reader contract drift"):
                    SCOPE.validate_frontend(base, head, base, [ROOT_LAYOUT])

    def test_reader_mode_and_missing_reader_are_not_the_pinned_blob(self):
        original = self.base
        for mode in ("executable", "missing"):
            with self.subTest(mode=mode):
                self.base = original
                self.restore()
                if mode == "executable":
                    Path(READER).chmod(0o755)
                else:
                    Path(READER).unlink()
                self.base = self.commit()
                self.edit(ROOT_LAYOUT)
                result = self.classify()
                self.assert_full(result)
                self.assertIn("disjoint reader contract drift", result["reason"])

    def test_child_symlink_to_root_cannot_masquerade_as_disjoint(self):
        Path(CHILD_LAYOUT).unlink()
        Path(CHILD_LAYOUT).symlink_to("../_layout.tsx")
        self.base = self.commit()
        self.edit(ROOT_LAYOUT)
        result = self.classify()
        self.assert_full(result)
        self.assertIn("disjoint reader contract drift", result["reason"])

    def test_unknown_direct_basename_and_dynamic_readers_remain_full(self):
        original = self.base
        for path, body in (
            ("tools/reader.py", f'INPUT = "{ROOT_LAYOUT}"\n'),
            ("crates/fixture/src/lib.rs", 'const INPUT: &str = "_layout.tsx";\n'),
            ("crates/fixture/src/lib.rs", 'const ROOT: &str = "apps/zoen/src";\n'),
            ("miniapps/other/test/reader.mjs", self.reader),
        ):
            with self.subTest(path=path, body=str(body)[:50]):
                self.base = original
                self.restore()
                self.write(path, body)
                self.base = self.commit()
                self.edit(ROOT_LAYOUT)
                self.assert_full(self.classify())

    def test_selector_or_own_test_changes_still_require_full(self):
        for path in ("scripts/ci-documentation-scope.py",
                     "scripts/tests/test_ci_frontend_disjoint_scope.py"):
            with self.subTest(path=path):
                self.restore()
                self.edit(ROOT_LAYOUT)
                self.write(path, "# Changed selector/test\n")
                self.assert_full(self.classify())


if __name__ == "__main__":
    unittest.main()
