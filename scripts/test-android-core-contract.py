#!/usr/bin/env python3
"""Exercise stale-source, missing-API and mixed-ABI rejection without a native build."""

import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("android_contract", Path(__file__).with_name("android-core-contract.py"))
core = importlib.util.module_from_spec(spec)
spec.loader.exec_module(core)

SOURCE = """
#[derive(uniffi::Object)]
pub struct ZoenEngine { inner: PrivateImplementation }
#[derive(uniffi::Record)]
#[expect(clippy::large_enum_variant, reason = "source attribute")]
pub struct PlanPageDto { pub total_cents: Option<i64>, pub legacy_edit_admitted: bool }
#[uniffi::export]
impl ZoenEngine {
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
        self.output = self.root / "android/app/build/generated/zoen"
        self.addCleanup(patch.stopall)
        patch.object(core, "revision", return_value={"commit": "fixture-main", "branch": "main", "label": "working-tree-dirty"}).start()
        for name in ["Cargo.toml", "Cargo.lock", "android/uniffi.toml", "scripts/build-android-core.sh",
                     "scripts/fix-android-uniffi-callbacks.py", "scripts/android-core-contract.py"]:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture\n")
        self.source = self.root / "crates/zoen-ffi/src/lib.rs"
        self.source.parent.mkdir(parents=True)
        self.source.write_text(SOURCE)
        self.snapshot = core.capture(self.root)
        self.contract = self.snapshot["contract"]
        self.metadata = {name: (name + shape).encode() for name, shape in self.contract["metadata"].items()}
        self.kotlin = self.output / "kotlin/xyz/tironi/zoen/core/zoen_ffi.kt"
        self.kotlin.parent.mkdir(parents=True)
        self.kotlin.write_text("\n".join(f"if ((lib.{name}() and 0xFFFF) != {index}) {{ error(\"ABI\") }}"
                                          for index, name in enumerate(self.contract["checksums"])))
        self.abis = ["arm64-v8a", "x86_64"]
        for abi in self.abis:
            write_elf(self.output / "jniLibs" / abi / "libzoen_ffi.so", abi, self.metadata, self.contract["checksums"])
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
        missing = core.CHECKSUM_PREFIX + "method_zoenengine_plan_page"
        self.kotlin.write_text("\n".join(line for line in self.kotlin.read_text().splitlines() if missing not in line))
        with self.assertRaisesRegex(ValueError, "Kotlin ABI: missing exports"):
            core.verify(self.root, self.output, self.abis)
        obsolete = dict(self.metadata)
        del obsolete[core.META_PREFIX + "METHOD_ZOENENGINE_PLAN_PAGE"]
        write_elf(self.output / "jniLibs/arm64-v8a/libzoen_ffi.so", "arm64-v8a", obsolete, self.contract["checksums"])
        blobs, _ = core.elf_contract(self.output / "jniLibs/arm64-v8a/libzoen_ffi.so", "arm64-v8a")
        with self.assertRaisesRegex(ValueError, "missing exports"):
            core.require_inventory(blobs, self.contract["metadata"], "JNI")

    def test_same_named_but_stale_dto_abi_and_modified_binary_are_rejected(self):
        altered = {**self.metadata, core.META_PREFIX + "RECORD_PLANPAGEDTO": b"obsolete-signed-cents-layout"}
        native = self.output / "jniLibs/x86_64/libzoen_ffi.so"
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
        native = self.output / "jniLibs/x86_64/libzoen_ffi.so"
        write_elf(native, "arm64-v8a", self.metadata, self.contract["checksums"])
        with self.assertRaisesRegex(ValueError, "expected a 64-bit x86_64"):
            core.verify(self.root, self.output, self.abis)
        native.unlink()
        with self.assertRaises(FileNotFoundError):
            core.verify(self.root, self.output, self.abis)

    def test_callbacks_constructor_and_attributes_are_inventory_inputs(self):
        self.assertIn(core.META_PREFIX + "INTERFACE_SECRETVAULT", self.contract["metadata"])
        self.assertIn(core.META_PREFIX + "RECORD_PLANPAGEDTO", self.contract["metadata"])
        self.assertIn(core.CHECKSUM_PREFIX + "constructor_zoenengine_open", self.contract["checksums"])
        fake = '\nconst EXAMPLE: &str = "#[uniffi::export] impl Fake { pub fn absent(&self) {} }";'
        self.assertEqual(self.contract, core.source_contract({"src/lib.rs": SOURCE + fake}))


if __name__ == "__main__":
    unittest.main()
