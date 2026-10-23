"""Real Git/Cargo-metadata fixtures; no compilation, services or devices."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/expo-artifact-scope.py"
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("expo_scope", SCRIPT)
SCOPE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SCOPE)
CARGO = shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo")
PACKAGES = {
    "client-core": "engines/phone", "uniffi-bindgen": "tools/generator",
    "zoen-web-core": "engines/browser", "zoen-relay": "services/relay",
    "zoen-cli": "tools/client", "shared": "common/contracts",
    "native-extra": "engines/extra", "unrelated": "services/unrelated",
}


class Repository:
    def __init__(self, directory):
        self.root = Path(directory)
        self.root.mkdir(parents=True)
        self.git("init", "-q")
        self.git("config", "user.name", "Scope test")
        self.git("config", "user.email", "scope@example.invalid")
        self.write("Cargo.toml", '[workspace]\nresolver="2"\nmembers=' + json.dumps(list(PACKAGES.values())) + "\n")
        for name, path in PACKAGES.items():
            self.write(path + "/Cargo.toml", f'[package]\nname="{name}"\nversion="0.1.0"\nedition="2021"\n')
            self.write(path + "/src/lib.rs", "pub fn original() {}\n")
        self.dependency("client-core", "shared")
        self.dependency("zoen-relay", "shared")
        self.dependency("zoen-web-core", "shared")
        self.dependency("zoen-cli", "client-core")
        for path in SCOPE.CONTRACTS:
            self.write(path, (ROOT / path).read_text())
        self.base = self.commit("baseline")

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.root, check=True,
                              capture_output=True, text=True).stdout.strip()

    def write(self, path, text):
        destination = self.root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text)

    def dependency(self, source, dependency, section="dependencies", extra="", rename=None):
        path = self.root / PACKAGES[source] / "Cargo.toml"
        relative = os.path.relpath(self.root / PACKAGES[dependency], path.parent)
        text = path.read_text()
        header = f'\n[{section}]\n'
        entry = f'{rename or dependency}={{path="{relative}"' + (f',package="{dependency}"' if rename else "") + extra + "}\n"
        if header in text:
            position = text.index(header) + len(header)
            text = text[:position] + entry + text[position:]
        else:
            text += header + entry
        path.write_text(text)

    def commit(self, message):
        self.git("add", ".")
        self.git("commit", "-qm", message)
        return self.git("rev-parse", "HEAD")

    def change(self, package="zoen-relay"):
        self.write(PACKAGES[package] + "/src/lib.rs", "pub fn changed() {}\n")
        return self.commit("Rust change")

    def classify(self, head=None, event="pull_request", cargo=CARGO):
        return SCOPE.classify(self.base, head or self.git("rev-parse", "HEAD"), event, cargo)

    def broken_cargo(self, mode):
        program = self.root.parent / ("cargo-" + mode)
        code = "import json,os,subprocess\n"
        if mode == "invalid-json":
            code += 'print("not metadata")\n'
        else:
            code += "p=json.loads(subprocess.check_output(" + repr([CARGO, "metadata", "--no-deps", "--offline", "--locked", "--format-version", "1"]) + "))\n"
            if mode == "incomplete":
                code += "p['packages']=[v for v in p['packages'] if v['name']!='client-core']\n"
            elif mode == "outside":
                code += "p['packages'][0]['dependencies'].append({'kind':None,'path':'/outside','source':None})\n"
            elif mode == "unknown-local":
                code += "p['packages'][0]['dependencies'].append({'kind':None,'path':os.getcwd()+'/missing','source':None})\n"
            elif mode == "bad-root":
                code += "p['workspace_root']='/outside'\n"
            elif mode == "build-input":
                code += "p['packages'][0]['targets'][0]['kind']=['custom-build']\n"
            elif mode == "unknown-kind":
                code += "p['packages'][0]['targets'][0]['kind']=['unknown-target-kind']\n"
            code += "print(json.dumps(p))\n"
        program.write_text("#!" + sys.executable + "\n" + code)
        program.chmod(0o700)
        return str(program)


class ScopeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.repo = Repository(Path(self.temporary.name) / "repo")
        self.old_cwd = Path.cwd()
        os.chdir(self.repo.root)
        self.addCleanup(os.chdir, self.old_cwd)

    def full(self, result):
        self.assertTrue(all(result["run_" + job] for job in SCOPE.JOBS), result)

    def verified_full(self, result):
        self.full(result)
        self.assertTrue(result["reason"].startswith("Verified"), result)

    def test_relay_source_skips_independent_artifacts_but_runs_its_actual_web_journey(self):
        head = self.repo.change()
        result = self.repo.classify(head)
        self.assertFalse(result["run_android"], result)
        self.assertFalse(result["run_client"], result)
        self.assertTrue(result["run_release_web"], result)
        self.assertIn("zoen-relay", result["packages"]["release_web"])
        self.assertNotIn("zoen-relay", result["packages"]["android"])
        self.assertFalse((self.repo.root / "target").exists(), "metadata must not compile")
        pushed = self.repo.classify(head, event="push")
        self.assertEqual([pushed["run_" + j] for j in SCOPE.JOBS], [False, False, True])

    def test_shared_dependency_selects_all_artifacts_without_prefix_assumptions(self):
        self.repo.change("shared")
        self.verified_full(self.repo.classify())

    def test_indirect_renamed_optional_and_target_edges_remain_conservative(self):
        cases = [
            ("dependencies", ',optional=true', "renamed_relay"),
            ('target.\'cfg(target_os = "android")\'.dependencies', "", None),
        ]
        for section, extra, rename in cases:
            with self.subTest(section=section):
                path = self.repo.root / PACKAGES["client-core"] / "Cargo.toml"
                original = path.read_text()
                self.repo.dependency("client-core", "zoen-relay", section, extra, rename)
                self.repo.base = self.repo.commit("edge baseline")
                self.repo.change()
                self.verified_full(self.repo.classify())
                path.write_text(original)
                self.repo.write(PACKAGES["zoen-relay"] + "/src/lib.rs", "pub fn original() {}\n")
                self.repo.base = self.repo.commit("restore")
        self.repo.dependency("client-core", "native-extra")
        self.repo.dependency("native-extra", "zoen-relay")
        self.repo.base = self.repo.commit("indirect baseline")
        self.repo.change()
        self.verified_full(self.repo.classify())

    def test_types_exporter_includes_dev_edges_while_android_preserves_builder_contract(self):
        self.repo.dependency("client-core", "zoen-relay", "dev-dependencies")
        self.repo.base = self.repo.commit("dev baseline")
        self.repo.change()
        result = self.repo.classify()
        self.assertFalse(result["run_android"], result)
        self.assertTrue(result["run_client"] and result["run_release_web"], result)

    def test_metadata_reads_the_committed_tree_not_uncommitted_worktree_manifests(self):
        manifest = self.repo.root / PACKAGES["client-core"] / "Cargo.toml"
        original = manifest.read_text()
        self.repo.dependency("client-core", "zoen-relay", "dev-dependencies")
        self.repo.base = self.repo.commit("dependency baseline")
        self.repo.change()
        manifest.write_text(original)
        result = self.repo.classify()
        self.assertFalse(result["run_android"], result)
        self.assertTrue(result["run_client"] and result["run_release_web"], result)

    def test_literal_foreign_compiler_or_js_reader_cannot_be_hidden_by_metadata(self):
        for index, spacing in enumerate(["", " ", "\n  "]):
            with self.subTest(spacing=spacing):
                self.repo.write(PACKAGES["client-core"] + "/src/lib.rs",
                                'const SOURCE: &str = include_str' + spacing +
                                '!("../../../services/relay/src/lib.rs");\n')
                self.repo.base = self.repo.commit("compiler consumer baseline")
                self.repo.write(PACKAGES["zoen-relay"] + "/src/lib.rs", f"pub fn changed_{index}() {{}}\n")
                self.repo.commit("Relay change")
                self.verified_full(self.repo.classify())
        self.repo.write(PACKAGES["client-core"] + "/src/lib.rs", "pub fn original() {}\n")
        self.repo.write("apps/zoen/src/reader.ts", "readFile('services/relay/src/lib.rs');\n")
        self.repo.base = self.repo.commit("JS consumer baseline")
        self.repo.write(PACKAGES["zoen-relay"] + "/src/lib.rs", "pub fn changed_again() {}\n")
        self.repo.commit("Relay change")
        self.verified_full(self.repo.classify())

    def test_dynamic_include_and_missing_input_fall_back_without_compilation(self):
        for text in ['include_str!(concat!("a", "b"));', 'include_str!("missing.rs");',
                     'include_str /* comment */ !("file.rs");', 'include_str!["file.rs"];']:
            with self.subTest(text=text):
                self.repo.write(PACKAGES["client-core"] + "/src/lib.rs", text)
                self.repo.base = self.repo.commit("unmodelled input baseline")
                self.repo.write(PACKAGES["zoen-relay"] + "/src/lib.rs", text + " // changed")
                self.repo.commit("Relay source")
                self.full(self.repo.classify())

    def test_manifest_lock_config_tools_unknown_and_mixed_inputs_require_full(self):
        for path in ["Cargo.lock", ".cargo/config.toml", "notes.txt", "stray/unknown.rs", "scripts/build-expo-core.mjs",
                     "services/relay/build.rs", "services/relay/Cargo.toml"]:
            with self.subTest(path=path):
                self.repo.write(path, "changed unknown input\n")
                self.repo.commit("unknown input")
                self.full(self.repo.classify())
                self.repo.git("reset", "--hard", self.repo.base)
        self.repo.change()
        self.repo.write("unexpected.txt", "mixed")
        self.repo.commit("mixed")
        self.full(self.repo.classify())

    def test_delete_rename_symlink_and_both_mode_directions_require_full(self):
        path = PACKAGES["zoen-relay"] + "/src/lib.rs"
        self.repo.git("mv", path, path.replace("lib.rs", "renamed.rs"))
        self.repo.commit("rename")
        self.full(self.repo.classify())
        self.repo.git("reset", "--hard", self.repo.base)
        for mode in (0o755, 0o644):
            (self.repo.root / path).chmod(0o644 if mode == 0o755 else 0o755)
            self.repo.base = self.repo.commit("mode baseline") if mode == 0o644 else self.repo.base
            (self.repo.root / path).chmod(mode)
            self.repo.commit("mode change")
            self.full(self.repo.classify())
            self.repo.git("reset", "--hard", self.repo.base)
        (self.repo.root / path).unlink()
        self.repo.commit("delete")
        self.full(self.repo.classify())
        self.repo.git("reset", "--hard", self.repo.base)
        (self.repo.root / path).unlink()
        (self.repo.root / path).symlink_to("elsewhere.rs")
        self.repo.commit("symlink")
        self.full(self.repo.classify())

    def test_contract_drift_in_either_endpoint_or_worktree_cannot_suppress_jobs(self):
        workflow = ".github/workflows/expo.yml"
        self.repo.write(workflow, (self.repo.root / workflow).read_text() + "\n# changed commands\n")
        self.repo.base = self.repo.commit("drift baseline")
        self.repo.write(workflow, (ROOT / workflow).read_text())
        head = self.repo.change()
        self.full(self.repo.classify(head))
        self.repo.git("reset", "--hard", self.repo.base)
        self.repo.change()
        self.full(self.repo.classify())

    def test_missing_unknown_and_failed_metadata_parsers_never_select_a_partial_lane(self):
        self.repo.change()
        for mode in ["invalid-json", "incomplete", "outside", "unknown-local", "bad-root", "build-input", "unknown-kind"]:
            with self.subTest(mode=mode):
                self.full(self.repo.classify(cargo=self.repo.broken_cargo(mode)))
        self.full(self.repo.classify(cargo="/missing-cargo-executable"))
        with patch.object(SCOPE, "changes", side_effect=ValueError("bad raw diff")):
            self.full(self.repo.classify())
        for raw in [b"broken", b":100644 100644 x y\0file.rs\0", b":100644 100644 x y M\0bad\xff.rs\0"]:
            with patch.object(SCOPE, "git", return_value=raw), self.assertRaises(ValueError):
                SCOPE.changes("base", "head")

    def test_unknown_paths_nonancestor_push_merge_ambiguity_and_empty_refs_require_full(self):
        self.full(self.repo.classify())
        self.full(SCOPE.classify("missing", "HEAD", "pull_request", CARGO))
        self.full(self.repo.classify(event="unknown_event"))
        tree = self.repo.git("rev-parse", "HEAD^{tree}")
        a = self.repo.git("commit-tree", tree, "-p", self.repo.base, "-m", "a")
        b = self.repo.git("commit-tree", tree, "-p", self.repo.base, "-m", "b")
        a2 = self.repo.git("commit-tree", tree, "-p", a, "-p", b, "-m", "a2")
        b2 = self.repo.git("commit-tree", tree, "-p", b, "-p", a, "-m", "b2")
        self.full(SCOPE.classify(a2, b2, "pull_request", CARGO))
        self.full(SCOPE.classify(a, b, "push", CARGO))

    def test_cli_outputs_are_honest_and_failure_conditions_keep_full_jobs_selected(self):
        head = self.repo.change()
        output = self.repo.root.parent / "outputs"
        summary = self.repo.root.parent / "summary"
        result = subprocess.run([sys.executable, str(SCRIPT), "--base", self.repo.base,
                                 "--head", head, "--event", "pull_request", "--cargo", CARGO],
                                cwd=self.repo.root, capture_output=True, text=True, check=True,
                                env={**os.environ, "GITHUB_OUTPUT": str(output), "GITHUB_STEP_SUMMARY": str(summary)})
        self.assertFalse(json.loads(result.stdout)["run_android"])
        self.assertIn("run_android=false\n", output.read_text())
        self.assertIn("run_release_web=true\n", output.read_text())
        self.assertIn("SKIPPED", summary.read_text())
        self.assertNotIn("PASS", summary.read_text())
        workflow = (ROOT / ".github/workflows/expo.yml").read_text()
        for job in SCOPE.JOBS:
            self.assertIn("!cancelled() && (needs.artifact-scope.result != 'success' || needs.artifact-scope.outputs.run_" + job + " != 'false')", workflow)
        self.assertIn("cargo build --locked -p zoen-relay -p zoen-cli", workflow)


if __name__ == "__main__":
    unittest.main()
