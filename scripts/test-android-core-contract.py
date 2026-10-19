#!/usr/bin/env python3
"""Exercise stale-source, missing-API and mixed-ABI rejection without a native build."""

import importlib.util
import contextlib
import gzip
import io
import json
from pathlib import Path
import runpy
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("android_contract", Path(__file__).with_name("android-core-contract.py"))
core = importlib.util.module_from_spec(spec)
spec.loader.exec_module(core)
upstream_spec = importlib.util.spec_from_file_location("android_upstream", Path(__file__).with_name("check-android-upstream.py"))
upstream = importlib.util.module_from_spec(upstream_spec)
upstream_spec.loader.exec_module(upstream)

SOURCE = """
#[derive(uniffi::Object)]
pub struct Client { inner: PrivateImplementation }
#[derive(uniffi::Record)]
#[expect(clippy::large_enum_variant, reason = "source attribute")]
pub struct PlanPageDto { pub total_cents: Option<i64>, pub legacy_edit_admitted: bool }
#[uniffi::export]
impl Client {
    #[uniffi::constructor]
    pub fn open(path: String) -> Self { unimplemented!() }
    pub fn plan_page(&self, id: String) -> PlanPageDto { unimplemented!() }
}
#[uniffi::export(with_foreign)]
pub trait SecretVault: Send + Sync { fn load(&self, key: String) -> Option<Vec<u8>>; }
"""


def write_elf(path, abi, metadata, checksums):
    """A loader-shaped ELF fixture with real export-table metadata, no executable code."""
    names = bytearray(b"\0")
    symbols = bytearray(24)
    blobs = bytearray()
    for name in sorted(set(metadata) | set(checksums)):
        name_offset = len(names)
        names.extend(name.encode() + b"\0")
        blob = metadata.get(name, b"")
        symbols.extend(struct.pack("<IBBHQQ", name_offset, 0x11 if blob else 0x12, 0, 3, 0x1000 + len(blobs), len(blob)))
        blobs.extend(blob)
    names_offset = 64 + 4 * 64
    symbols_offset = names_offset + len(names)
    blobs_offset = symbols_offset + len(symbols)
    header = struct.pack("<16sHHIQQQIHHHHHH", b"\x7fELF\x02\x01\x01" + b"\0" * 9,
                         3, core.ABI_MACHINES[abi], 1, 0, 0, 64, 0, 64, 0, 0, 64, 4, 0)
    section = lambda kind, address, offset, length, link=0, size=0: struct.pack("<IIQQQQIIQQ", 0, kind, 0, address, offset, length, link, 0, 1, size)
    sections = bytes(64) + section(3, 0, names_offset, len(names)) + section(11, 0, symbols_offset, len(symbols), 1, 24) + section(1, 0x1000, blobs_offset, len(blobs))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(header + sections + names + symbols + blobs)


class CurrentCoreContractTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.output = self.root / "android/app/build/generated/core"
        self.addCleanup(patch.stopall)
        patch.object(core, "revision", return_value={"commit": "fixture-main", "branch": "main", "label": "working-tree-dirty"}).start()
        for name in ["Cargo.toml", "Cargo.lock", "android/uniffi.toml", "scripts/build-android-core.sh",
                     "scripts/fix-android-uniffi-callbacks.py", "scripts/android-core-contract.py"]:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture\n")
        (self.root / "Cargo.toml").write_text('[workspace]\nmembers = ["crates/*"]\n')
        self.source = self.root / "crates/client-core/src/lib.rs"
        self.source.parent.mkdir(parents=True)
        self.source.write_text(SOURCE)
        (self.source.parent.parent / "Cargo.toml").write_text(
            '[package]\nname = "client-core"\nversion = "0.0.0"\n'
            '[lib]\nname = "client_core"\ncrate-type = ["cdylib", "staticlib"]\n'
            '[dependencies]\nuniffi = "0.31"\n')
        self.snapshot = core.capture(self.root)
        self.contract = self.snapshot["contract"]
        self.metadata = {name: (name + shape).encode() for name, shape in self.contract["metadata"].items()}
        self.kotlin = self.output / "kotlin/xyz/tironi/zoen/core/client_core.kt"
        self.kotlin.parent.mkdir(parents=True)
        self.kotlin.write_text("\n".join(f"if ((lib.{name}() and 0xFFFF) != {index}) {{ error(\"ABI\") }}"
                                          for index, name in enumerate(self.contract["checksums"])))
        self.abis = ["arm64-v8a", "x86_64"]
        for abi in self.abis:
            write_elf(self.output / "jniLibs" / abi / "libclient_core.so", abi, self.metadata, self.contract["checksums"])
        generated = core.artifacts(self.output, self.abis, self.contract)
        manifest = {**self.snapshot, "abis": self.abis, "profile": "release", "artifacts": generated,
                    "runtime_validation": "fixture has no runtime acceptance"}
        (self.output / "core-contract.json").write_text(json.dumps(manifest))

    def test_current_source_verifies_and_ui_followup_does_not_freeze_core(self):
        self.assertEqual("matching-source-and-generated-ABI", core.verify(self.root, self.output, self.abis, "release")["status"])
        ui = self.root / "apple/Shared/Features/SpaceView.swift"
        ui.parent.mkdir(parents=True)
        ui.write_text("new current-main UI\n")
        self.assertEqual(self.snapshot["input_sha256"], core.capture(self.root)["input_sha256"])
        self.assertEqual("matching-source-and-generated-ABI", core.verify(self.root, self.output, self.abis)["status"])

    def test_changed_signed_cents_type_requires_new_generation(self):
        self.source.write_text(SOURCE.replace("Option<i64>", "Option<i32>"))
        with self.assertRaisesRegex(ValueError, "different source inputs"):
            core.verify(self.root, self.output, self.abis)

    def test_missing_exported_plan_api_is_rejected_in_kotlin_and_jni(self):
        missing = core.CHECKSUM_PREFIX + "method_client_plan_page"
        self.kotlin.write_text("\n".join(line for line in self.kotlin.read_text().splitlines() if missing not in line))
        with self.assertRaisesRegex(ValueError, "Kotlin ABI: missing exports"):
            core.verify(self.root, self.output, self.abis)
        obsolete = dict(self.metadata)
        del obsolete[core.META_PREFIX + "METHOD_CLIENT_PLAN_PAGE"]
        write_elf(self.output / "jniLibs/arm64-v8a/libclient_core.so", "arm64-v8a", obsolete, self.contract["checksums"])
        blobs, _ = core.elf_contract(self.output / "jniLibs/arm64-v8a/libclient_core.so", "arm64-v8a")
        with self.assertRaisesRegex(ValueError, "missing exports"):
            core.require_inventory(blobs, self.contract["metadata"], "JNI")

    def test_same_named_but_stale_dto_abi_and_modified_binary_are_rejected(self):
        altered = {**self.metadata, core.META_PREFIX + "RECORD_PLANPAGEDTO": b"obsolete-signed-cents-layout"}
        native = self.output / "jniLibs/x86_64/libclient_core.so"
        write_elf(native, "x86_64", altered, self.contract["checksums"])
        with self.assertRaisesRegex(ValueError, "stale native UniFFI contract"):
            core.verify(self.root, self.output, self.abis)
        write_elf(native, "x86_64", self.metadata, self.contract["checksums"])
        native.write_bytes(native.read_bytes() + b"changed-producer-artifact")
        with self.assertRaisesRegex(ValueError, "changed after the source snapshot"):
            core.verify(self.root, self.output, self.abis)

    def test_missing_abi_wrong_architecture_or_profile_cannot_certify_release(self):
        with self.assertRaisesRegex(ValueError, "ABIs/profile"):
            core.verify(self.root, self.output, self.abis, "dev")
        native = self.output / "jniLibs/x86_64/libclient_core.so"
        write_elf(native, "arm64-v8a", self.metadata, self.contract["checksums"])
        with self.assertRaisesRegex(ValueError, "expected a 64-bit x86_64"):
            core.verify(self.root, self.output, self.abis)
        native.unlink()
        with self.assertRaises(FileNotFoundError):
            core.verify(self.root, self.output, self.abis)

    def test_callbacks_constructor_and_attributes_are_inventory_inputs(self):
        self.assertIn(core.META_PREFIX + "INTERFACE_SECRETVAULT", self.contract["metadata"])
        self.assertIn(core.META_PREFIX + "RECORD_PLANPAGEDTO", self.contract["metadata"])
        self.assertIn(core.CHECKSUM_PREFIX + "constructor_client_open", self.contract["checksums"])
        fake = '\nconst EXAMPLE: &str = "#[uniffi::export] impl Fake { pub fn absent(&self) {} }";'
        self.assertEqual(self.contract, core.source_contract({"src/lib.rs": SOURCE + fake}))

    def test_kotlin_checksum_namespace_must_match_the_current_library(self):
        self.kotlin.write_text(self.kotlin.read_text().replace(core.CHECKSUM_PREFIX, "uniffi_other_core_checksum_"))
        with self.assertRaisesRegex(ValueError, "Missing generated Kotlin ABI checksum inventory"):
            core.verify(self.root, self.output, self.abis)

    def test_immutable_baseline_discovers_its_own_manifest_after_library_path_and_name_change(self):
        def command(*args):
            return subprocess.check_output(["git", "-C", str(self.root), *args], text=True).strip()
        command("init", "-q")
        command("add", ".")
        command("-c", "user.name=Contract Test", "-c", "user.email=contract@example.invalid",
                "commit", "-qm", "original library")
        baseline = command("rev-parse", "HEAD")
        target = self.root / "engines/renamed-bindings"
        target.parent.mkdir()
        self.source.parent.parent.rename(target)
        (self.root / "Cargo.toml").write_text('[workspace]\nmembers = ["engines/*"]\n')
        manifest = target / "Cargo.toml"
        manifest.write_text(manifest.read_text().replace('name = "client_core"', 'name = "renamed_client"')
                            .replace('[lib]\n', '[lib]\npath = "bindings/entry.rs"\n'))
        (target / "src").rename(target / "bindings")
        (target / "bindings/lib.rs").rename(target / "bindings/entry.rs")
        original = core.library_at(self.root, baseline)
        current = core.library_at(self.root)
        self.assertEqual("crates/client-core/Cargo.toml", original["library"]["manifest"])
        self.assertEqual("client_core", original["library"]["name"])
        self.assertEqual("engines/renamed-bindings/bindings/entry.rs", current["library"]["entry"])
        self.assertEqual("renamed_client", current["library"]["name"])
        self.assertIn("UNIFFI_META_CLIENT_CORE_INTERFACE_CLIENT", original["contract"]["metadata"])
        self.assertIn("UNIFFI_META_RENAMED_CLIENT_INTERFACE_CLIENT", current["contract"]["metadata"])

    def test_raw_delta_reports_the_actual_namespace_break_without_normalizing_exports(self):
        previous = core.source_contract({"src/lib.rs": SOURCE}, "previous_native")
        current = core.source_contract({"src/lib.rs": SOURCE}, "client_core")
        delta = upstream.contract_delta(previous, current)
        self.assertIn("UNIFFI_META_PREVIOUS_NATIVE_INTERFACE_CLIENT", delta["removed"])
        self.assertIn("UNIFFI_META_CLIENT_CORE_INTERFACE_CLIENT", delta["added"])
        self.assertEqual([], delta["changed"])
        self.assertEqual(len(previous["metadata"]), len(delta["removed"]))

    def test_immutable_workspace_glob_does_not_match_an_unrelated_nested_library(self):
        unrelated = self.root / "unrelated/crates/foreign"
        (unrelated / "src").mkdir(parents=True)
        (unrelated / "Cargo.toml").write_text(
            '[package]\nname = "foreign"\nversion = "0.0.0"\n'
            '[lib]\ncrate-type = ["cdylib"]\n[dependencies]\nuniffi = "0.31"\n')
        (unrelated / "src/lib.rs").write_text(SOURCE)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        subprocess.run(["git", "-C", str(self.root), "add", "."], check=True)
        subprocess.run(["git", "-C", str(self.root), "-c", "user.name=Contract Test",
                        "-c", "user.email=contract@example.invalid", "commit", "-qm", "nested nonmember"], check=True)
        ref = subprocess.check_output(["git", "-C", str(self.root), "rev-parse", "HEAD"], text=True).strip()
        self.assertEqual("crates/client-core/Cargo.toml", core.library_at(self.root, ref)["library"]["manifest"])

    def test_library_discovery_rejects_ambiguous_workspace_instead_of_guessing(self):
        second = self.root / "crates/second"
        (second / "src").mkdir(parents=True)
        (second / "Cargo.toml").write_text(
            '[package]\nname = "second"\nversion = "0.0.0"\n'
            '[lib]\ncrate-type = ["cdylib"]\n[dependencies]\nuniffi = "0.31"\n')
        (second / "src/lib.rs").write_text(SOURCE)
        with self.assertRaisesRegex(ValueError, "exactly one UniFFI cdylib"):
            core.library_at(self.root)


class OriginalArtEquivalenceTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.directory = self.root / "android/app/src/test/resources"
        (self.directory / "source-equivalence").mkdir(parents=True)
        self.path = "apple/Original.swift"
        source = self.root / self.path
        source.parent.mkdir()
        source.write_text("original independent source\n")
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        subprocess.run(["git", "-C", str(self.root), "add", "."], check=True)
        subprocess.run(["git", "-C", str(self.root), "-c", "user.name=Art Test",
                        "-c", "user.email=art@example.invalid", "commit", "-qm", "actual source"], check=True)
        ref = subprocess.check_output(["git", "-C", str(self.root), "rev-parse", "HEAD"], text=True).strip()
        self.original = {self.path: core.digest(source.read_bytes())}
        self.payload = core.json_digest({"points": [[0.2, 0.3]]})
        self.proof = {"verified_ref": ref, "reference_payload_sha256": self.payload,
                      "source_variants": {self.path: [self.original[self.path]]}}
        self.save()

    def save(self):
        target = self.directory / "source-equivalence/proof.json"
        target.write_text(json.dumps(self.proof))
        (self.directory / "art-source-equivalence.json").write_text(json.dumps({
            "schema": 1, "proofs": {"ink": [{"path": "source-equivalence/proof.json",
                                              "sha256": core.digest(target.read_bytes())}]}}))

    def verify(self, payload=None):
        return upstream.verified_art_variants(self.root, "ink", payload or self.payload, self.original)

    def test_exact_source_proof_cannot_accept_a_changed_geometry_payload(self):
        self.assertEqual({self.path: {self.original[self.path]}}, self.verify())
        with self.assertRaisesRegex(ValueError, "actual producer/corpus"):
            self.verify(core.json_digest({"points": [[0.21, 0.3]]}))

    def test_rehashed_receipt_still_cannot_authorize_an_unproved_source_variant(self):
        self.proof["source_variants"][self.path].append("0" * 64)
        self.save()
        with self.assertRaisesRegex(ValueError, "not proved by its immutable source"):
            self.verify()

    def test_mutable_git_ref_or_changed_receipt_is_rejected(self):
        self.proof["verified_ref"] = "HEAD"
        self.save()
        with self.assertRaisesRegex(ValueError, "immutable source revisions"):
            self.verify()
        (self.directory / "source-equivalence/proof.json").write_text("{}")
        with self.assertRaisesRegex(ValueError, "actual producer/corpus"):
            self.verify()


class SwiftProducerReferenceTest(unittest.TestCase):
    def test_mutable_reference_is_resolved_before_all_source_reads(self):
        root = Path(__file__).resolve().parents[1]
        commit = "1" * 40
        cases = [
            ("export-android-ink-reference.py", "ink-swift-reference.json.gz", None),
            ("export-android-ink-icons.py", "ink-icons-swift-reference.json.gz", "icons"),
        ]
        for producer, corpus, payload_key in cases:
            with self.subTest(producer=producer), tempfile.TemporaryDirectory() as directory:
                reference = json.loads(gzip.decompress((root / "android/app/src/test/resources" / corpus).read_bytes()))
                payload = reference[payload_key] if payload_key else {key: value for key, value in reference.items() if key != "sources"}
                calls = []

                def run(command, **kwargs):
                    calls.append(command)
                    if command[:2] == ["git", "rev-parse"]:
                        self.assertEqual(command, ["git", "rev-parse", "--verify", "moving-branch^{commit}"])
                        return subprocess.CompletedProcess(command, 0, stdout=commit + "\n")
                    if command[:2] == ["git", "show"]:
                        self.assertTrue(command[2].startswith(commit + ":"))
                        return subprocess.CompletedProcess(command, 0, stdout=(root / command[2].split(":", 1)[1]).read_bytes())
                    if command[0] == "swiftc":
                        return subprocess.CompletedProcess(command, 0)
                    self.assertIn(Path(command[0]).name, ("reference", "icons"))
                    return subprocess.CompletedProcess(command, 0, stdout=json.dumps(payload).encode())

                output = Path(directory) / "proof.json"
                with patch.object(sys, "argv", [producer, "--verify-ref", "moving-branch", "--equivalence-output", str(output)]), \
                        patch.object(subprocess, "run", side_effect=run), contextlib.redirect_stdout(io.StringIO()):
                    with self.assertRaises(SystemExit) as stopped:
                        runpy.run_path(str(root / "scripts" / producer), run_name="__main__")
                self.assertEqual(stopped.exception.code, 0)
                self.assertEqual(calls[0], ["git", "rev-parse", "--verify", "moving-branch^{commit}"])
                self.assertEqual(sum(command[:2] == ["git", "rev-parse"] for command in calls), 1)
                self.assertEqual(json.loads(output.read_text())["verified_ref"], commit)


if __name__ == "__main__":
    unittest.main()
