#!/usr/bin/env python3
"""Bind Android's generated UniFFI contract and JNI artifacts to their source inputs."""

import argparse
import ctypes
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "android/app/build/generated/zoen"
ABI_MACHINES = {"arm64-v8a": 183, "x86_64": 62}
META_PREFIX = "UNIFFI_META_ZOEN_FFI_"
CHECKSUM_PREFIX = "uniffi_zoen_ffi_checksum_"


def digest(value):
    return hashlib.sha256(value).hexdigest()


ART_RENAME = Path(__file__).with_name("art-rename-equivalence.json")


def _renamed(original):
    return re.sub(r"\broda\b", "zoen", original.replace("Roda", "Zoen"))


def art_rename_lines():
    lines = json.loads(ART_RENAME.read_text())["lines"]
    for renamed, original in lines.items():
        if renamed == original or _renamed(original) != renamed:
            raise ValueError("Art rename equivalence must only rename Roda to Zoen: " + renamed)
    return {renamed.encode(): original.encode() for renamed, original in lines.items()}


def art_bytes(value, lines=None):
    """Map whole lines renamed Roda -> Zoen back to the original art source.

    Only exact lines listed in art-rename-equivalence.json are rewritten; every other
    byte must still match the original artwork, so the art gate stays strict.
    """
    lines = art_rename_lines() if lines is None else lines
    return b"\n".join(lines.get(line, line) for line in value.split(b"\n"))


def json_digest(value):
    return digest(json.dumps(value, sort_keys=True, separators=(",", ":")).encode())


def git(root, *args, check=True):
    return subprocess.run(["git", *args], cwd=root, check=check, capture_output=True, text=True)


def revision(root):
    dirty = bool(git(root, "status", "--porcelain", "--untracked-files=all").stdout)
    return {"commit": git(root, "rev-parse", "HEAD").stdout.strip(),
            "branch": git(root, "branch", "--show-current").stdout.strip(),
            "label": "working-tree-dirty" if dirty else "committed-source"}


def input_hashes(root):
    fixed = ["Cargo.toml", "Cargo.lock", "android/uniffi.toml",
             "scripts/build-android-core.sh", "scripts/fix-android-uniffi-callbacks.py",
             "scripts/android-core-contract.py"]
    paths = {root / name for name in fixed}
    for directory in [root / "crates", root / "tools/uniffi-bindgen"]:
        paths.update(path for path in directory.rglob("*") if path.is_file()
                     and path.suffix in {".rs", ".toml", ".html", ".json"}
                     and not {"target", "build", "node_modules"}.intersection(path.relative_to(root).parts))
    return {path.relative_to(root).as_posix(): digest(path.read_bytes()) for path in sorted(paths)}


def mask_rust(source):
    return re.sub(r'//[^\n]*|/\*.*?\*/|r(?P<hash>#+|)".*?"(?P=hash)|"(?:\\.|[^"\\])*"',
                  lambda match: " " * len(match[0]), source, flags=re.S)


def body_end(masked, opening):
    depth = 1
    for index in range(opening + 1, len(masked)):
        depth += (masked[index] == "{") - (masked[index] == "}")
        if depth == 0:
            return index
    raise ValueError("Unclosed exported Rust definition")


def source_contract(sources):
    """Inventory every exported object, callback, function, record, enum and error."""
    metadata, checksums = {}, set()
    for path, source in sources.items():
        if not path.endswith(".rs") or "tests" in Path(path).parts or path.endswith("/tests.rs"):
            continue
        masked = mask_rust(source)
        for derived in re.finditer(r"#\[derive\([^\]]*uniffi::(Record|Enum|Error|Object)[^\]]*\)\]\s*(?:#\[[^\]]*\]\s*)*pub\s+(?:struct|enum)\s+(\w+)\s*\{", masked):
            kind, name = derived[1], derived[2]
            opening = derived.end() - 1
            shape = masked[opening + 1:body_end(masked, opening)]
            metadata[META_PREFIX + ("INTERFACE" if kind == "Object" else kind.upper()) + "_" + name.upper()] = (
                "object" if kind == "Object" else digest(re.sub(r"\s+", "", shape).encode()))
        for exported in re.finditer(r"#\[uniffi::export(?:\([^\]]*\))?\]\s*(?:impl\s+(\w+)|pub\s+trait\s+(\w+)[^{]*)\s*\{", masked):
            owner = exported[1] or exported[2]
            trait = bool(exported[2])
            if trait:
                metadata[META_PREFIX + "INTERFACE_" + owner.upper()] = "callback"
            opening = exported.end() - 1
            body = masked[opening + 1:body_end(masked, opening)]
            for method in re.finditer(r"(?P<constructor>#\[uniffi::constructor\]\s*)?(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*(\([^{}]*?\)[^{};]*?)[{;]", body):
                kind = "constructor" if method["constructor"] else "method"
                name = f"{kind}_{owner.lower()}_{method[2]}"
                symbol = META_PREFIX + name.upper()
                if symbol in metadata:
                    raise ValueError("Duplicate exported method: " + symbol)
                metadata[symbol] = digest(re.sub(r"\s+", "", method[3]).encode())
                checksums.add(CHECKSUM_PREFIX + name)
        for exported in re.finditer(r"#\[uniffi::export(?:\([^\]]*\))?\]\s*pub\s+(?:async\s+)?fn\s+(\w+)\s*(\([^{}]*?\)[^{}]*?)\{", masked):
            metadata[META_PREFIX + "FUNC_" + exported[1].upper()] = digest(re.sub(r"\s+", "", exported[2]).encode())
            checksums.add(CHECKSUM_PREFIX + "func_" + exported[1])
    if not metadata or not checksums:
        raise ValueError("No exported UniFFI contract found")
    return {"metadata": dict(sorted(metadata.items())), "checksums": sorted(checksums)}


def contract_at(root, ref=None):
    if ref is None:
        sources = {path.relative_to(root).as_posix(): path.read_text()
                   for path in (root / "crates/zoen-ffi/src").rglob("*.rs")}
    else:
        paths = git(root, "ls-tree", "-r", "--name-only", ref, "crates/zoen-ffi/src").stdout.splitlines()
        legacy = not paths
        if legacy:  # revisions before the roda -> zoen rename
            paths = git(root, "ls-tree", "-r", "--name-only", ref, "crates/roda-ffi/src").stdout.splitlines()
        sources = {path: git(root, "show", ref + ":" + path).stdout for path in paths if path.endswith(".rs")}
        if legacy:
            sources = {path.replace("crates/roda-ffi/", "crates/zoen-ffi/"):
                       re.sub(r"Roda(?=[A-Z])", "Zoen", re.sub(r"\broda_", "zoen_", text))
                       for path, text in sources.items()}
    return source_contract(sources)


def capture(root):
    inputs, contract = input_hashes(root), contract_at(root)
    return {"schema": 1, "source": revision(root), "inputs": inputs,
            "input_sha256": json_digest(inputs), "contract": contract,
            "contract_sha256": json_digest(contract)}


def elf_contract(path, abi):
    """Read metadata from Android ELF without loading or executing a JNI library."""
    data = path.read_bytes()
    if data[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<H", data, 18)[0] != ABI_MACHINES[abi]:
        raise ValueError(f"{path}: expected a 64-bit {abi} ELF library")
    offset = struct.unpack_from("<Q", data, 40)[0]
    entry_size, count = struct.unpack_from("<HH", data, 58)
    sections = [struct.unpack_from("<IIQQQQIIQQ", data, offset + index * entry_size) for index in range(count)]
    metadata, checksums = {}, set()
    for section in sections:
        if section[1] != 11:  # SHT_DYNSYM: only exports accessible to the loader.
            continue
        strings = sections[section[6]]
        names = data[strings[4]:strings[4] + strings[5]]
        for index in range(section[5] // section[9]):
            name_offset, info, _, section_index, value, size = struct.unpack_from("<IBBHQQ", data, section[4] + index * section[9])
            if not section_index or info >> 4 not in (1, 2):
                continue
            name = names[name_offset:names.find(b"\0", name_offset)].decode()
            if name.startswith(CHECKSUM_PREFIX):
                checksums.add(name)
            if name.startswith(META_PREFIX):
                owner = sections[section_index]
                start = owner[4] + value - owner[3]
                blob = data[start:start + size]
                if not size or len(blob) != size:
                    raise ValueError("Invalid UniFFI metadata export: " + name)
                metadata[name] = blob
    if not metadata:
        raise ValueError(f"{path}: missing UniFFI metadata")
    return metadata, checksums


def kotlin_checksums(directory):
    files = sorted(directory.rglob("*.kt"))
    if not files:
        raise ValueError("Missing generated Kotlin bindings")
    checksums = {}
    for path in files:
        for symbol, value in re.findall(r"\(lib\.(uniffi_zoen_ffi_checksum_\w+)\(\) and 0xFFFF\) != (\d+)", path.read_text()):
            if symbol in checksums:
                raise ValueError("Duplicate Kotlin ABI checksum: " + symbol)
            checksums[symbol] = int(value)
    if not checksums:
        raise ValueError("Missing generated Kotlin ABI checksum inventory")
    return checksums


def require_inventory(actual, expected, label):
    missing, obsolete = sorted(set(expected) - set(actual)), sorted(set(actual) - set(expected))
    if missing or obsolete:
        raise ValueError(f"{label}: missing exports {missing}; obsolete exports {obsolete}")


def artifacts(output, abis, contract, expected_metadata=None):
    generated = {path.relative_to(output).as_posix(): digest(path.read_bytes()) for path in sorted((output / "kotlin").rglob("*.kt"))}
    checksums = kotlin_checksums(output / "kotlin")
    require_inventory(checksums, contract["checksums"], "Kotlin ABI")
    metadata = None
    for abi in abis:
        path = output / "jniLibs" / abi / "libzoen_ffi.so"
        blobs, symbols = elf_contract(path, abi)
        require_inventory(blobs, contract["metadata"], f"{abi} DTO/callback/API metadata")
        require_inventory(symbols, contract["checksums"], f"{abi} ABI")
        current = {name: digest(blob) for name, blob in blobs.items()}
        if current != (expected_metadata if expected_metadata is not None else metadata or current):
            raise ValueError(f"{abi}: stale native UniFFI contract")
        metadata = current
        generated[path.relative_to(output).as_posix()] = digest(path.read_bytes())
    return {"files": generated, "metadata": metadata, "checksums": checksums}


def finish(root, output, source, host_library, abis, profile):
    current = capture(root)
    if source["input_sha256"] != current["input_sha256"] or source["contract_sha256"] != current["contract_sha256"]:
        raise ValueError("Core inputs changed during Android generation; rebuild the stable source snapshot")
    generated = artifacts(output, abis, current["contract"])
    host = ctypes.CDLL(str(host_library.resolve()))
    blobs, _ = elf_contract(output / "jniLibs" / abis[0] / "libzoen_ffi.so", abis[0])
    for name, blob in blobs.items():
        native = bytes((ctypes.c_ubyte * len(blob)).in_dll(host, name))
        if native != blob:
            raise ValueError("Host bindgen input and Android metadata differ: " + name)
    for name, expected in generated["checksums"].items():
        method = getattr(host, name)
        method.restype = ctypes.c_uint16
        method.argtypes = []
        if method() != expected:
            raise ValueError("Generated Kotlin checksum differs from host bindgen input: " + name)
    return {**current, "build_source": source["source"], "abis": abis, "profile": profile,
            "host_library_sha256": digest(host_library.read_bytes()), "artifacts": generated,
            "runtime_validation": "not established by source, ABI metadata or artifact hashes"}


def verify(root, output, abis, profile=None):
    path = output / "core-contract.json"
    manifest = json.loads(path.read_text())
    current = capture(root)
    if manifest.get("schema") != 1 or manifest["input_sha256"] != current["input_sha256"] or manifest["contract_sha256"] != current["contract_sha256"]:
        raise ValueError("Generated Android core belongs to different source inputs; rebuild Kotlin and JNI together")
    if not set(abis).issubset(manifest["abis"]) or profile and manifest["profile"] != profile:
        raise ValueError("Generated Android core does not contain the requested ABIs/profile")
    actual = artifacts(output, manifest["abis"], current["contract"], manifest["artifacts"]["metadata"])
    if actual != manifest["artifacts"]:
        raise ValueError("Generated Kotlin or JNI changed after the source snapshot was recorded")
    return {"status": "matching-source-and-generated-ABI", "manifest": str(path),
            "source": manifest["source"], "current_source": current["source"], "input_sha256": manifest["input_sha256"],
            "contract_sha256": manifest["contract_sha256"], "abis": manifest["abis"],
            "profile": manifest["profile"], "runtime_validation": manifest["runtime_validation"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["capture", "finish", "verify"])
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    parser.add_argument("--host-library", type=Path)
    parser.add_argument("--abis", default="arm64-v8a,x86_64")
    parser.add_argument("--profile", choices=["dev", "release"])
    args = parser.parse_args()
    abis = args.abis.split(",")
    if not abis or len(set(abis)) != len(abis) or set(abis) - ABI_MACHINES.keys():
        parser.error("Use arm64-v8a and/or x86_64 once each")
    try:
        if args.mode == "capture":
            args.output.mkdir(parents=True, exist_ok=True)
            (args.output / "core-inputs.json").write_text(json.dumps(capture(args.root), indent=2) + "\n")
            (args.output / "core-contract.json").unlink(missing_ok=True)
        elif args.mode == "finish":
            if args.host_library is None or args.profile is None:
                parser.error("finish requires --host-library and --profile")
            source = json.loads((args.output / "core-inputs.json").read_text())
            manifest = finish(args.root, args.output, source, args.host_library, abis, args.profile)
            (args.output / "core-contract.json").write_text(json.dumps(manifest, indent=2) + "\n")
        else:
            print(json.dumps(verify(args.root, args.output, abis, args.profile), indent=2))
    except (OSError, ValueError, KeyError, struct.error) as error:
        raise SystemExit(str(error)) from error


if __name__ == "__main__":
    main()
