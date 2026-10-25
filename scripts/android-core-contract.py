#!/usr/bin/env python3
"""Bind Android's generated UniFFI contract and JNI artifacts to their source inputs."""

import argparse
import ctypes
import hashlib
import json
from pathlib import Path
from pathlib import PurePosixPath
import posixpath
import re
import struct
import subprocess
try:
    import tomllib
except ModuleNotFoundError:
    try:
        import tomli as tomllib
    except ModuleNotFoundError as error:
        raise SystemExit("Python <3.11 requires tomli. Activate a tool virtual environment and run "
                         "python -m pip install -r scripts/requirements-python.txt") from error


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "apps/zoen/modules/zoen-core/generated/android"
KOTLIN_CONFIG = "apps/zoen/modules/zoen-core/android/uniffi.toml"
ABI_MACHINES = {"arm64-v8a": 183, "x86_64": 62}
META_PREFIX = "UNIFFI_META_CLIENT_CORE_"
CHECKSUM_PREFIX = "uniffi_client_core_checksum_"
BUILD_INPUTS = [
    "scripts/build-android-core.sh", "scripts/fix-android-uniffi-callbacks.py",
    "scripts/android-core-contract.py", "scripts/expo-android-guarded.mjs",
    "scripts/expo-android-cargo-proxy.mjs", "scripts/expo-cargo-artifacts.mjs",
    "scripts/cargo-source-guard.py", "scripts/cargo_guard_configuration.py",
]


def digest(value):
    return hashlib.sha256(value).hexdigest()


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
    fixed = ["Cargo.toml", "Cargo.lock", KOTLIN_CONFIG, *BUILD_INPUTS]
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


def source_contract(sources, library_name="client_core"):
    """Inventory every exported object, callback, function, record, enum and error."""
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", library_name):
        raise ValueError("Invalid UniFFI library name")
    meta_prefix = f"UNIFFI_META_{library_name.upper()}_"
    checksum_prefix = f"uniffi_{library_name}_checksum_"
    metadata, checksums = {}, set()
    for path, source in sources.items():
        if not path.endswith(".rs") or "tests" in Path(path).parts or path.endswith("/tests.rs"):
            continue
        masked = mask_rust(source)
        for derived in re.finditer(r"#\[derive\([^\]]*uniffi::(Record|Enum|Error|Object)[^\]]*\)\]\s*(?:#\[[^\]]*\]\s*)*pub\s+(?:struct|enum)\s+(\w+)\s*\{", masked):
            kind, name = derived[1], derived[2]
            opening = derived.end() - 1
            shape = masked[opening + 1:body_end(masked, opening)]
            metadata[meta_prefix + ("INTERFACE" if kind == "Object" else kind.upper()) + "_" + name.upper()] = (
                "object" if kind == "Object" else digest(re.sub(r"\s+", "", shape).encode()))
        for exported in re.finditer(r"#\[uniffi::export(?:\([^\]]*\))?\]\s*(?:impl\s+(\w+)|pub\s+trait\s+(\w+)[^{]*)\s*\{", masked):
            owner = exported[1] or exported[2]
            trait = bool(exported[2])
            if trait:
                metadata[meta_prefix + "INTERFACE_" + owner.upper()] = "callback"
            opening = exported.end() - 1
            body = masked[opening + 1:body_end(masked, opening)]
            for method in re.finditer(r"(?P<attributes>(?:#\[[^\]]*\]\s*)*)(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*(\([^{}]*?\)[^{};]*?)[{;]", body):
                constructor = re.search(r"#\[\s*uniffi::constructor\s*\]", method["attributes"])
                kind = "constructor" if constructor else "method"
                name = f"{kind}_{owner.lower()}_{method[2]}"
                symbol = meta_prefix + name.upper()
                if symbol in metadata:
                    raise ValueError("Duplicate exported method: " + symbol)
                metadata[symbol] = digest(re.sub(r"\s+", "", method[3]).encode())
                checksums.add(checksum_prefix + name)
        for exported in re.finditer(r"#\[uniffi::export(?:\([^\]]*\))?\]\s*pub\s+(?:async\s+)?fn\s+(\w+)\s*(\([^{}]*?\)[^{}]*?)\{", masked):
            metadata[meta_prefix + "FUNC_" + exported[1].upper()] = digest(re.sub(r"\s+", "", exported[2]).encode())
            checksums.add(checksum_prefix + "func_" + exported[1])
    if not metadata or not checksums:
        raise ValueError("No exported UniFFI contract found")
    return {"metadata": dict(sorted(metadata.items())), "checksums": sorted(checksums)}


def library_at(root, ref=None):
    """Read the actual workspace cdylib manifest, including at an immutable Git ref.

    A historical revision may place its library anywhere in its workspace. No
    current path, package name, or checked-out file is substituted for that tree.
    """
    root = Path(root).resolve()
    read = (lambda path: (root / path).read_text()) if ref is None else (
        lambda path: git(root, "show", ref + ":" + path).stdout)
    workspace = tomllib.loads(read("Cargo.toml")).get("workspace", {})
    members = workspace.get("members", [])
    excludes = workspace.get("exclude", [])
    if ref is None:
        manifests = {path.relative_to(root).as_posix() + "/Cargo.toml"
                     for pattern in members for path in root.glob(pattern)
                     if path.is_dir() and (path / "Cargo.toml").is_file()}
    else:
        manifests = {path for path in git(root, "ls-tree", "-r", "--name-only", ref).stdout.splitlines()
                     if path.endswith("/Cargo.toml")}
    candidates = []
    for path in sorted(manifests):
        member = str(PurePosixPath(path).parent)
        matches = lambda pattern: PurePosixPath("/" + member).match("/" + posixpath.normpath(pattern))
        if not any(matches(pattern) for pattern in members) or any(matches(pattern) for pattern in excludes):
            continue
        manifest = tomllib.loads(read(path))
        package = manifest.get("package", {})
        library = manifest.get("lib", {})
        dependencies = manifest.get("dependencies", {})
        uniffi = any(name == "uniffi" or isinstance(value, dict) and value.get("package") == "uniffi"
                     for name, value in dependencies.items())
        if not uniffi or "cdylib" not in library.get("crate-type", []):
            continue
        name = library.get("name", package.get("name", "").replace("-", "_"))
        entry = posixpath.normpath(posixpath.join(member, library.get("path", "src/lib.rs")))
        if entry.startswith("../") or entry.startswith("/") or not entry.endswith(".rs"):
            raise ValueError("UniFFI library target must be a Rust source inside the workspace")
        candidates.append({"manifest": path, "package": package["name"], "name": name,
                           "entry": entry, "source_directory": str(PurePosixPath(entry).parent)})
    if len(candidates) != 1:
        raise ValueError(f"Expected exactly one UniFFI cdylib workspace library at {ref or 'working tree'}; found {len(candidates)}")
    library = candidates[0]
    # Confirm that the manifest's target exists, rather than accepting a nearby source directory.
    read(library["entry"])
    if ref is None:
        paths = [path.relative_to(root).as_posix()
                 for path in (root / library["source_directory"]).rglob("*.rs")]
    else:
        paths = git(root, "ls-tree", "-r", "--name-only", ref, library["source_directory"]).stdout.splitlines()
    sources = {path: read(path) for path in paths if path.endswith(".rs")}
    return {"library": library, "sources": sources,
            "contract": source_contract(sources, library["name"])}


def contract_at(root, ref=None):
    return library_at(root, ref)["contract"]


def contract_delta(baseline, current):
    before, after = baseline["metadata"], current["metadata"]
    return {"added": sorted(after.keys() - before.keys()),
            "removed": sorted(before.keys() - after.keys()),
            "changed": sorted(name for name in before.keys() & after.keys() if before[name] != after[name])}


def capture(root):
    inputs, snapshot = input_hashes(root), library_at(root)
    contract = snapshot["contract"]
    return {"schema": 1, "source": revision(root), "inputs": inputs,
            "library": snapshot["library"], "input_sha256": json_digest(inputs), "contract": contract,
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
        for symbol, value in re.findall(rf"\(lib\.({re.escape(CHECKSUM_PREFIX)}\w+)\(\) and 0xFFFF\) != (\d+)", path.read_text()):
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
        path = output / "jniLibs" / abi / "libclient_core.so"
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
    blobs, _ = elf_contract(output / "jniLibs" / abis[0] / "libclient_core.so", abis[0])
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
    if "cargo_qualification" in manifest:
        reference = manifest["cargo_qualification"]
        raw = Path(reference["path"]).read_bytes()
        qualification = json.loads(raw)
        if digest(raw) != reference["sha256"] or qualification.get("schema") != "zoen.android.cargo-qualification.v1" or qualification.get("sourceSha256") != current["input_sha256"]:
            raise ValueError("Android Cargo qualification changed or belongs to different inputs")
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
    parser.add_argument("--cargo-qualification", type=Path)
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
            if args.cargo_qualification is not None:
                raw = args.cargo_qualification.read_bytes()
                qualification = json.loads(raw)
                if qualification.get("schema") != "zoen.android.cargo-qualification.v1" or qualification.get("sourceSha256") != source["input_sha256"]:
                    raise ValueError("Android Cargo qualification belongs to different source inputs")
                manifest["cargo_qualification"] = {"path": str(args.cargo_qualification.resolve()), "sha256": digest(raw)}
            (args.output / "core-contract.json").write_text(json.dumps(manifest, indent=2) + "\n")
        else:
            print(json.dumps(verify(args.root, args.output, abis, args.profile), indent=2))
    except (OSError, ValueError, KeyError, struct.error) as error:
        raise SystemExit(str(error)) from error


if __name__ == "__main__":
    main()
