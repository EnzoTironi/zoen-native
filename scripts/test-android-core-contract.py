#!/usr/bin/env python3
"""Exercise stale-source, missing-API and mixed-ABI rejection without a native build."""

import importlib.util
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("android_contract", Path(__file__).with_name("android-core-contract.py"))
core = importlib.util.module_from_spec(spec)
spec.loader.exec_module(core)

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
        self.output = self.root / "apps/zoen/modules/zoen-core/generated/android"
        self.addCleanup(patch.stopall)
        patch.object(core, "revision", return_value={"commit": "fixture-main", "branch": "main", "label": "working-tree-dirty"}).start()
        for name in ["Cargo.toml", "Cargo.lock", core.KOTLIN_CONFIG, *core.BUILD_INPUTS]:
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
        ui = self.root / "apps/zoen/src/app/chat/[id].tsx"
        ui.parent.mkdir(parents=True)
        ui.write_text("new current-main UI\n")
        self.assertEqual(self.snapshot["input_sha256"], core.capture(self.root)["input_sha256"])
        self.assertEqual("matching-source-and-generated-ABI", core.verify(self.root, self.output, self.abis)["status"])

    def test_shared_cargo_qualification_reference_must_remain_bound_to_source_and_bytes(self):
        reference = self.root / "private-qualification.json"
        reference.write_text(json.dumps({"schema": "zoen.android.cargo-qualification.v1",
                                        "sourceSha256": self.snapshot["input_sha256"]}))
        manifest_path = self.output / "core-contract.json"
        manifest = json.loads(manifest_path.read_text())
        manifest["cargo_qualification"] = {"path": str(reference),
                                            "sha256": core.digest(reference.read_bytes())}
        manifest_path.write_text(json.dumps(manifest))
        self.assertEqual("matching-source-and-generated-ABI",
                         core.verify(self.root, self.output, self.abis)["status"])
        reference.write_text(reference.read_text() + " ")
        with self.assertRaisesRegex(ValueError, "qualification changed"):
            core.verify(self.root, self.output, self.abis)
        reference.write_text(json.dumps({"schema": "zoen.android.cargo-qualification.v1",
                                        "sourceSha256": "0" * 64}))
        manifest["cargo_qualification"]["sha256"] = core.digest(reference.read_bytes())
        manifest_path.write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, "different inputs"):
            core.verify(self.root, self.output, self.abis)
        reference.unlink()
        with self.assertRaises(FileNotFoundError):
            core.verify(self.root, self.output, self.abis)

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

    def test_constructor_keeps_its_kind_with_other_rust_attributes(self):
        for attributes in (
            '#[must_use]\n    #[uniffi::constructor]',
            '#[uniffi::constructor]\n    #[must_use]',
            '#[uniffi::constructor]\n    #[doc = "first launch"]\n    #[must_use]',
        ):
            with self.subTest(attributes=attributes):
                source = SOURCE.replace('#[uniffi::constructor]', attributes)
                contract = core.source_contract({'src/lib.rs': source})
                self.assertEqual(self.contract, contract)
                self.assertNotIn(core.CHECKSUM_PREFIX + 'method_client_open', contract['checksums'])

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
        delta = core.contract_delta(previous, current)
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

    def test_active_kotlin_configuration_invalidates_bindings_without_legacy_client(self):
        self.assertFalse((self.root / "android").exists())
        config = self.root / core.KOTLIN_CONFIG
        config.write_text(config.read_text() + "# changed active binding policy\n")
        self.assertNotEqual(self.snapshot["input_sha256"], core.capture(self.root)["input_sha256"])
        with self.assertRaisesRegex(ValueError, "different source inputs"):
            core.verify(self.root, self.output, self.abis)




if __name__ == "__main__":
    unittest.main()
