"""Exact standalone image inputs retain native qualification, not Rust journeys."""
from pathlib import Path
import re
import subprocess

from ci_frontend_scope_fixture import FrontendRepositoryFixture, SCOPE, SOURCE


class PhotoHelperScopeTests(FrontendRepositoryFixture):
    def setUp(self):
        super().setUp()
        for path in SCOPE.PHOTO_HELPER_PATHS:
            self.write(path, (SOURCE.parents[1] / path).read_bytes())
        self.base = self.commit()

    def edit_photo(self):
        path = sorted(SCOPE.PHOTO_HELPER_PATHS)[0]
        self.write(path, Path(path).read_text() + "\n// Geometry change\n")
        return path

    def test_each_exact_swift_input_and_both_select_native_not_rust(self):
        for paths in ([path] for path in sorted(SCOPE.PHOTO_HELPER_PATHS)):
            self.git("reset", "--hard", self.base)
            for path in paths:
                self.write(path, Path(path).read_text() + "\n// Pixel change\n")
            result = self.classify()
            self.assertTrue(result.get("photo_helper_only", False), result)
            self.assertFalse(result["run_rust"], result)
            self.assertTrue(result["run_photo_helper"], result)
            self.assertFalse(result["frontend_only"], result)
        self.edit_photo()
        self.assertTrue(self.classify()["photo_helper_only"])

    def test_current_666_pencilkit_fixture_is_used_without_substitution(self):
        # setUp copies the maintained sources verbatim, including #666's actual
        # PKDrawing/extended-range image reproduction, not a synthetic RGBA stand-in.
        fixture = next(path for path in SCOPE.PHOTO_HELPER_PATHS if "/tests/" in path)
        maintained = (SOURCE.parents[1] / fixture).read_bytes()
        self.assertEqual(Path(fixture).read_bytes(), maintained)
        self.assertIn(b"import AppKit", maintained)
        self.assertIn(b"import PencilKit", maintained)
        self.assertIn(b"PKDrawing(strokes:", maintained)
        self.assertIn(b"try actualPencilKitRecrop()", maintained)
        self.assertIn(b"try explicitExtendedRangeRecrop()", maintained)
        self.assertIn(b"CGColorSpace.extendedSRGB", maintained)
        self.edit_photo()
        result = self.classify()
        self.assertTrue(result["photo_helper_only"], result)
        self.assertFalse(result["run_rust"], result)
        self.assertTrue(result["run_photo_helper"], result)

    def test_reviewed_fixture_imports_never_expand_helper_or_admit_unknown_modules(self):
        helper = next(path for path in SCOPE.PHOTO_HELPER_PATHS if "/ios/" in path)
        fixture = next(path for path in SCOPE.PHOTO_HELPER_PATHS if "/tests/" in path)
        for path, addition in ((helper, "\nimport PencilKit\n"),
                               (fixture, "\nimport UIKit\n"),
                               (fixture, "\nimport ZoenCore\n"),
                               (fixture, '\n@_silgen_name("core_call") func coreCall()\n')):
            with self.subTest(path=path, addition=addition):
                self.git("reset", "--hard", self.base)
                self.write(path, Path(path).read_text() + addition)
                self.assert_full(self.classify())

    def test_any_other_input_keeps_all_rust(self):
        for path in ("Cargo.toml", "Cargo.lock", "crates/fixture/src/lib.rs",
                     "apps/zoen/modules/zoen-core/ios/Other.swift",
                     "apps/zoen/modules/zoen-core/ios/ZoenCore.h",
                     "apps/zoen/src/core/generated/Command.ts",
                     "apps/zoen/src/core/driver.ts", "apps/zoen/src/app/screen.tsx",
                     "scripts/new-input.py", ".github/workflows/ci.yml"):
            with self.subTest(path=path):
                self.git("reset", "--hard", self.base)
                self.git("clean", "-fdq")
                self.edit_photo()
                self.write(path, "changed input\n")
                self.assert_full(self.classify())

    def test_native_fixture_or_workflow_drift_and_ffi_keep_rust(self):
        runner = "apps/zoen/src/core/photo-editor-image.test.ts"
        for path, addition in ((runner, "\n// Runner changed\n"),
                               (".github/workflows/ci.yml", "\n# Changed job\n")):
            self.git("reset", "--hard", self.base)
            self.write(path, Path(path).read_text() + addition)
            changed_base = self.commit()
            self.edit_photo()
            self.assert_full(self.classify(base=changed_base))
        for addition in ("\nimport ZoenCore\n", '\n@_silgen_name("core_call") func coreCall()\n',
                         '\nlet pointer = dlsym(nil, "core_call")\n'):
            self.git("reset", "--hard", self.base)
            path = self.edit_photo()
            self.write(path, Path(path).read_text() + addition)
            self.assert_full(self.classify())

    def test_unknown_readers_at_either_endpoint_keep_rust(self):
        photo = sorted(SCOPE.PHOTO_HELPER_PATHS)[0]
        for path, content in (("crates/fixture/src/lib.rs", f'const X: &str = include_str!("{photo}");'),
                              ("scripts/read.py", f'path = "{photo}"'),
                              ("crates/fixture/src/lib.rs", 'const X: &str = "apps/zoen/modules";')):
            self.git("reset", "--hard", self.base)
            self.write(path, content)
            changed_base = self.commit()
            self.edit_photo()
            self.assert_full(self.classify(base=changed_base))

    def test_push_invalid_history_modes_deletion_and_alias_fail_closed(self):
        self.edit_photo()
        self.assert_full(self.classify(event="push"))
        self.assert_full(self.classify(head=self.git("rev-parse", "HEAD"), base="0" * 40))
        path = sorted(SCOPE.PHOTO_HELPER_PATHS)[0]
        for kind in ("delete", "symlink", "executable"):
            self.git("reset", "--hard", self.base)
            if kind == "delete":
                Path(path).unlink()
            elif kind == "symlink":
                Path(path).unlink()
                Path(path).symlink_to("/tmp/foreign.swift")
            else:
                Path(path).chmod(0o755)
            self.assert_full(self.classify())

    def test_required_rust_check_demands_native_success_without_rust_shards(self):
        workflow = (SOURCE.parents[1] / ".github/workflows/ci.yml").read_text()
        rust = re.search(r"(?ms)^  rust:\n(.*?)(?=^  [a-z]|\Z)", workflow)[1]
        native = re.search(r"(?ms)^  photo-helper:\n(.*?)(?=^  [a-z]|\Z)", workflow)[1]
        self.assertIn("runs-on: macos-15", native)
        self.assertIn("node --test apps/zoen/src/core/photo-editor-image.test.ts", native)
        self.assertIn("--sdk iphonesimulator swiftc", native)
        self.assertIn("-warnings-as-errors", native)
        self.assertIn("photo_helper_only == 'true'", rust)
        gate = "\n".join(line[10:] for line in rust.split("        run: |\n", 1)[1].splitlines()
                         if line.startswith("          "))
        for result in ("success", "failure", "cancelled", "skipped", ""):
            run = subprocess.run(["sh", "-c", gate], env={**self.env, "PHOTO_ONLY": "true", "RUN_PHOTO": "true",
                                 "PHOTO_RESULT": result, "RUST_RESULT": "skipped"})
            self.assertEqual(run.returncode == 0, result == "success")
