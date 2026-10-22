"""Exercise the real Git index, commits and installer in disposable repositories."""

import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
HOOK = ROOT / ".githooks" / "pre-commit"
INSTALLER = ROOT / "scripts" / "install-git-hooks.py"
AUDIT = ROOT / "scripts" / "check-file-sizes.py"


class StagedFileLimitTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.repo = Path(self.temporary.name) / "repo"
        self.repo.mkdir()
        self.env = {key: value for key, value in os.environ.items()
                    if not key.startswith("GIT_")}
        self.env.update({
            "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_AUTHOR_NAME": "Hook Test", "GIT_AUTHOR_EMAIL": "hook@example.invalid",
            "GIT_COMMITTER_NAME": "Hook Test", "GIT_COMMITTER_EMAIL": "hook@example.invalid",
        })
        self.git("init", "--quiet")
        self.git("config", "commit.gpgsign", "false")
        (self.repo / ".githooks").mkdir()
        (self.repo / "scripts").mkdir()
        shutil.copyfile(HOOK, self.repo / ".githooks" / "pre-commit")
        shutil.copyfile(INSTALLER, self.repo / "scripts" / "install-git-hooks.py")
        if AUDIT.exists():
            shutil.copyfile(AUDIT, self.repo / "scripts" / "check-file-sizes.py")

    def run_command(self, args, check=True, cwd=None):
        result = subprocess.run(args, cwd=cwd or self.repo, env=self.env,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if check and result.returncode:
            self.fail("Command failed: {!r}\n{}".format(args, result.stderr.decode()))
        return result

    def git(self, *args, check=True):
        return self.run_command(["git", *args], check=check)

    def install(self, check=True):
        return self.run_command([sys.executable, "scripts/install-git-hooks.py"], check=check)

    def write(self, name, data, stage=True):
        path = self.repo / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        if stage:
            self.git("add", "--", name)
        return path

    def hook(self):
        # Use the interpreter explicitly so tests do not depend on checkout mode bits.
        return self.run_command([sys.executable, ".githooks/pre-commit"], check=False)

    def audit(self):
        return self.run_command([sys.executable, "scripts/check-file-sizes.py", "--json"],
                                check=False)

    def test_exact_boundary_and_explicit_split_instructions(self):
        self.write("source.py", b"a" * 20_000)
        self.assertEqual(self.hook().returncode, 0)
        self.write("source.py", b"a" * 20_001)
        result = self.hook()
        self.assertEqual(result.returncode, 1)
        message = result.stderr.decode()
        for fragment in ("20,001 bytes", "5,000.25 tokens", "sibling modules",
                         "tests by behavior", "Update imports", "git add", "Do not minify"):
            self.assertIn(fragment, message)

    def test_large_staged_blob_fails_after_worktree_shrinks(self):
        self.write("partial.rs", b"x" * 20_001)
        self.write("partial.rs", b"small", stage=False)
        self.assertEqual(self.hook().returncode, 1)

    def test_small_staged_blob_passes_after_worktree_grows(self):
        self.write("partial.rs", b"small")
        self.write("partial.rs", b"x" * 20_001, stage=False)
        self.assertEqual(self.hook().returncode, 0)

    def test_untracked_and_unstaged_files_are_ignored(self):
        self.write("tracked.py", b"small")
        self.git("commit", "--quiet", "-m", "seed")
        self.write("tracked.py", b"x" * 20_001, stage=False)
        self.write("untracked.py", b"x" * 20_001, stage=False)
        self.assertEqual(self.hook().returncode, 0)

    def test_unchanged_historical_file_is_not_checked(self):
        self.write("historical.py", b"x" * 20_001)
        self.git("commit", "--quiet", "-m", "pre-policy fixture")
        self.write("new.py", b"small")
        self.assertEqual(self.hook().returncode, 0)

    def test_modified_historical_file_is_checked(self):
        self.write("historical.py", b"x" * 20_001)
        self.git("commit", "--quiet", "-m", "pre-policy fixture")
        self.write("historical.py", b"y" * 20_001)
        self.assertEqual(self.hook().returncode, 1)

    def test_unicode_counts_bytes_not_characters(self):
        self.write("unicode.md", ("é" * 10_000).encode())
        self.assertEqual(self.hook().returncode, 0)
        self.write("unicode.md", ("é" * 10_001).encode())
        self.assertIn(b"20,002 bytes", self.hook().stderr)

    def test_explicit_artifact_exclusions(self):
        for name in (
            "Cargo.lock", "web/package-lock.json", "pnpm-lock.yaml", "Podfile.lock",
            "generated/types.rs", "src/__generated__/types.ts", "ClientFFI.generated.swift",
            "apps/zoen/ios/Podfile", "apps/zoen/android/build.gradle",
            "vendor/upstream.rs", "third_party/library.js", "node_modules/lib/index.js",
            "miniapps/deckcraft-ui/licenses/upstream.txt",
            "miniapps/deckcraft-ui/BOOTSTRAP-LICENSES.txt", "photo.png", "editor.wasm",
        ):
            with self.subTest(name=name):
                self.write(name, b"x" * 20_001)
                self.assertEqual(self.hook().returncode, 0)

    def test_authored_formats_and_patches_are_checked(self):
        for name in ("data.json", "icon.svg", "test_module.py", "docs/design.md",
                     "miniapps/deckcraft-ui/patches/change.patch"):
            with self.subTest(name=name):
                self.write(name, b"x" * 20_001)
                self.assertIn(ascii(name).encode(), self.hook().stderr)

    def test_generated_claim_in_authored_content_does_not_bypass(self):
        self.write("authored.ts", b"// @generated\n" + b"x" * 20_001)
        self.assertEqual(self.hook().returncode, 1)

    def test_binary_without_known_extension_is_skipped(self):
        self.write("asset.unknown", b"binary\0" + b"x" * 100_000)
        self.assertEqual(self.hook().returncode, 0)

    def test_large_binary_is_read_without_buffering_whole_file(self):
        self.write("asset.unknown", b"\0" + b"x" * (4 * 1024 * 1024))
        self.assertEqual(self.hook().returncode, 0)

    def test_rename_checks_new_authored_path(self):
        self.write("vendor/upstream.py", b"x" * 20_001)
        self.git("commit", "--quiet", "-m", "upstream fixture")
        self.git("mv", "vendor/upstream.py", "maintained.py")
        self.assertIn(b"maintained.py", self.hook().stderr)

    def test_deleted_large_file_is_skipped(self):
        self.write("historical.py", b"x" * 20_001)
        self.git("commit", "--quiet", "-m", "pre-policy fixture")
        self.git("rm", "historical.py")
        self.assertEqual(self.hook().returncode, 0)

    def test_symlink_does_not_follow_large_target(self):
        self.write("target.py", b"x" * 20_001, stage=False)
        (self.repo / "link.py").symlink_to("target.py")
        self.git("add", "link.py")
        self.assertEqual(self.hook().returncode, 0)

    def test_unusual_names_are_safe_and_all_reported(self):
        names = ["with spaces.py", "line\nbreak.py", "--option.py", "é.py", "escape\x1b.py"]
        for name in names:
            self.write(name, b"x" * 20_001)
        result = self.hook()
        self.assertEqual(result.returncode, 1)
        for name in names:
            self.assertIn(ascii(name).encode(), result.stderr)
        self.assertNotIn(b"\x1b", result.stderr)

    def test_duplicate_blob_paths_are_both_reported(self):
        self.write("one.rs", b"x" * 20_001)
        self.write("two.rs", b"x" * 20_001)
        result = self.hook()
        self.assertIn(b"one.rs", result.stderr)
        self.assertIn(b"two.rs", result.stderr)

    def test_executable_authored_file_is_checked(self):
        path = self.write("script", b"x" * 20_001, stage=False)
        path.chmod(0o755)
        self.git("add", "script")
        self.assertEqual(self.hook().returncode, 1)

    def test_actual_commit_blocked_then_succeeds_after_split(self):
        self.install()
        self.write("feature.py", b"x" * 20_001)
        failed = self.git("commit", "--quiet", "-m", "oversized", check=False)
        self.assertNotEqual(failed.returncode, 0)
        self.assertIn(b"Commit blocked", failed.stderr)
        self.write("feature.py", b"x" * 10_000)
        self.write("feature_helpers.py", b"x" * 10_001)
        self.git("commit", "--quiet", "-m", "split modules")
        self.assertEqual(self.git("rev-list", "--count", "HEAD").stdout.strip(), b"1")

    def test_install_is_idempotent_and_preserves_other_hooks(self):
        other = self.repo / ".git/hooks/pre-push"
        other.write_text("existing pre-push\n")
        self.install()
        self.install()
        installed = self.repo / ".git/hooks/pre-commit"
        self.assertEqual(installed.read_bytes(), HOOK.read_bytes())
        self.assertTrue(os.access(installed, os.X_OK))
        self.assertEqual(other.read_text(), "existing pre-push\n")

    def test_install_refuses_existing_different_hook(self):
        destination = self.repo / ".git/hooks/pre-commit"
        destination.write_text("existing hook\n")
        result = self.install(check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"Existing hook preserved", result.stderr)
        self.assertEqual(destination.read_text(), "existing hook\n")

    def test_install_refuses_hook_symlink(self):
        target = self.repo / "existing"
        target.write_text("preserve\n")
        (self.repo / ".git/hooks/pre-commit").symlink_to(target)
        self.assertNotEqual(self.install(check=False).returncode, 0)
        self.assertEqual(target.read_text(), "preserve\n")

    def test_custom_relative_hooks_path_is_honored(self):
        self.git("config", "core.hooksPath", "custom-hooks")
        self.install()
        self.assertEqual((self.repo / "custom-hooks/pre-commit").read_bytes(), HOOK.read_bytes())
        self.write("large.py", b"x" * 20_001)
        self.assertNotEqual(self.git("commit", "-m", "blocked", check=False).returncode, 0)

    def test_custom_absolute_hooks_path_is_honored(self):
        hooks = Path(self.temporary.name) / "absolute-hooks"
        self.git("config", "core.hooksPath", str(hooks))
        self.install()
        self.assertEqual((hooks / "pre-commit").read_bytes(), HOOK.read_bytes())

    def test_default_install_applies_to_linked_worktree(self):
        self.write("seed", b"small")
        self.git("commit", "--quiet", "-m", "seed")
        self.install()
        linked = Path(self.temporary.name) / "linked"
        self.git("worktree", "add", "--quiet", "-b", "linked", str(linked))
        (linked / "large.py").write_bytes(b"x" * 20_001)
        self.run_command(["git", "add", "large.py"], cwd=linked)
        result = self.run_command(["git", "commit", "-m", "blocked"], cwd=linked, check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"Commit blocked", result.stderr)

    def test_proven_artifact_outputs_are_exempt(self):
        for name in (
            "crates/miniapps/resources/excalidraw.html",
            "crates/miniapps/resources/deckcraft.html",
            "crates/miniapps/resources/pet.html", "LICENSE", "LICENSES/GPL-3.0.txt",
            "miniapps/excalidraw-ui/THIRD-PARTY-LICENSES.txt",
            "apps/zoen/modules/zoen-core/ios/EmojiKeyboard/ZoenEmojiKeyboardData.swift",
            "apps/zoen/modules/zoen-core/ios/EmojiKeyboard/emoji1.txt",
            "apps/zoen/modules/zoen-core/ios/TelegramAttachmentPresentation/Resources/Files.json",
        ):
            self.write(name, b"x" * 20_001)
        self.assertEqual(self.hook().returncode, 0)
        self.assertEqual(self.audit().returncode, 0)

    def test_artifact_exceptions_do_not_cover_authored_neighbors(self):
        for name in (
            "crates/miniapps/resources/authored.html",
            "miniapps/excalidraw-ui/src/canonical-app.tsx",
            "apps/zoen/modules/zoen-core/ios/EmojiKeyboard/generate-telegram-emoji.py",
            "apps/zoen/modules/zoen-core/ios/TelegramAttachmentPresentation/ZoenAdapter.swift",
            "apps/zoen/modules/zoen-core/ios/TelegramAttachmentPresentation/Resources/authored.json",
        ):
            self.write(name, b"x" * 20_001)
            self.assertIn(ascii(name).encode(), self.hook().stderr)

    def test_full_audit_finds_unchanged_historical_violation(self):
        import json
        self.write("historical.py", b"x" * 20_001)
        self.git("commit", "--quiet", "-m", "pre-policy fixture")
        self.assertEqual(self.hook().returncode, 0)
        result = self.audit()
        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["violationCount"], 1)
        self.assertEqual(report["violations"][0]["path"], "historical.py")
        self.assertEqual(report["violations"][0]["tokens"], 5000.25)

    def test_pinned_upstream_source_is_exempt_only_while_byte_exact(self):
        name = "miniapps/excalidraw-ui/src/sounds.ts"
        original = (ROOT / name).read_bytes()
        self.assertGreater(len(original), 20_000)
        self.write(name, original)
        self.assertEqual(self.hook().returncode, 0)
        self.assertEqual(self.audit().returncode, 0)
        self.write(name, original + b"\n// Local fork\n", stage=False)
        self.assertEqual(self.hook().returncode, 0, "only the staged import matters")
        self.git("add", name)
        self.assertEqual(self.hook().returncode, 1)
        self.assertEqual(self.audit().returncode, 1)

    def test_pinned_upstream_bytes_do_not_exempt_an_authored_neighbor(self):
        original = (ROOT / "miniapps/excalidraw-ui/src/sounds.ts").read_bytes()
        name = "miniapps/excalidraw-ui/src/local-sounds.ts"
        self.write(name, original)
        self.assertIn(ascii(name).encode(), self.hook().stderr)
        self.assertEqual(self.audit().returncode, 1)

    def test_pinned_upstream_filename_does_not_exempt_different_bytes(self):
        self.write("miniapps/excalidraw-ui/src/sounds.ts", b"x" * 20_001)
        self.assertEqual(self.hook().returncode, 1)
        self.assertEqual(self.audit().returncode, 1)

    def test_pinned_upstream_source_in_sha256_repository(self):
        shutil.rmtree(self.repo / ".git")
        self.git("init", "--quiet", "--object-format=sha256")
        name = "miniapps/excalidraw-ui/src/sounds.ts"
        self.write(name, (ROOT / name).read_bytes())
        self.assertEqual(self.hook().returncode, 0)
        self.assertEqual(self.audit().returncode, 0)
        self.write(name, (ROOT / name).read_bytes() + b"\n// Local fork\n")
        self.assertEqual(self.hook().returncode, 1)
        self.assertEqual(self.audit().returncode, 1)

    def test_full_audit_uses_same_index_boundary_and_exclusions(self):
        import json
        self.write("unicode.md", ("é" * 10_000).encode())
        self.write("Cargo.lock", b"x" * 20_001)
        self.write("binary.unknown", b"\0" + b"x" * 20_001)
        self.write("unicode.md", b"x" * 30_000, stage=False)
        result = self.audit()
        self.assertEqual(result.returncode, 0)
        self.assertEqual(json.loads(result.stdout)["violationCount"], 0)
        self.write("unicode.md", ("é" * 10_001).encode())
        self.assertEqual(self.audit().returncode, 1)

    def test_original_article_animation_is_pinned_in_both_git_formats(self):
        name = "apps/zoen/modules/zoen-core/ios/TelegramAttachmentPresentation/Resources/TabArticle.json"
        original = (ROOT / name).read_bytes()
        self.assertGreater(len(original), 20_000)
        for object_format in ("sha1", "sha256"):
            with self.subTest(object_format=object_format):
                shutil.rmtree(self.repo / ".git")
                self.git("init", "--quiet", "--object-format=" + object_format)
                self.write(name, original)
                self.assertEqual(self.hook().returncode, 0)
                self.assertEqual(self.audit().returncode, 0)
                self.write(name, original + b"\n", stage=False)
                self.assertEqual(self.hook().returncode, 0)
                self.git("add", name)
                self.assertEqual(self.hook().returncode, 1)
                self.assertEqual(self.audit().returncode, 1)
                self.write(name, original)
                neighbor = name.replace("TabArticle.json", "AuthoredArticle.json")
                self.write(neighbor, original)
                self.assertIn(ascii(neighbor).encode(), self.hook().stderr)
                self.assertEqual(self.audit().returncode, 1)
                self.git("rm", "--cached", neighbor)
                self.write(name, b"x" * 20_001)
                self.assertEqual(self.hook().returncode, 1)

    def test_full_audit_fails_closed_on_unmerged_index(self):
        self.write("source.py", b"initial\n")
        self.git("commit", "--quiet", "-m", "seed")
        self.git("checkout", "--quiet", "-b", "other")
        self.write("source.py", b"other\n")
        self.git("commit", "--quiet", "-m", "other")
        self.git("checkout", "--quiet", "-")
        self.write("source.py", b"current\n")
        self.git("commit", "--quiet", "-m", "current")
        self.assertNotEqual(self.git("merge", "other", check=False).returncode, 0)
        result = self.audit()
        self.assertEqual(result.returncode, 2)
        self.assertIn(b"unmerged index", result.stderr)


if __name__ == "__main__":
    unittest.main()
