#!/usr/bin/env python3
"""Verify current Main ancestry, original artwork and Android's generated Core contract."""

import argparse
import datetime
import gzip
import importlib.util
import json
from pathlib import Path
import re
import subprocess


ROOT = Path(__file__).resolve().parents[1]
MINIMUM_MAIN = "ff5b519f1734e9d28018ab33d3919d12c9147e45"
spec = importlib.util.spec_from_file_location("android_contract", ROOT / "scripts/android-core-contract.py")
core = importlib.util.module_from_spec(spec)
spec.loader.exec_module(core)


def contract_delta(baseline, current):
    before, after = baseline["metadata"], current["metadata"]
    return {"added": sorted(after.keys() - before.keys()),
            "removed": sorted(before.keys() - after.keys()),
            "changed": sorted(name for name in before.keys() & after.keys() if before[name] != after[name])}


def compare_exports(root, main_revision, source):
    """Report exact historical ABI changes; internal APIs may change prelaunch.

    The current-source Kotlin/JNI ABI check remains blocking after generation.
    No historical identifiers, paths, or object names are normalized or aliased.
    """
    baseline = core.library_at(root, main_revision)
    delta = contract_delta(baseline["contract"], source["contract"])
    return {"baseline_library": baseline["library"], "source_library": source["library"],
            "raw_abi_delta": delta,
            "raw_abi_compatible": not delta["removed"] and not delta["changed"],
            "policy": "Prelaunch internal API changes are report-only; current-source generated ABI, ancestry and artwork integrity remain blocking"}


def verified_art_variants(root, kind, payload_sha256, original_sources):
    """Accept source fingerprints only through exact producer/corpus receipts."""
    directory = root / "android/app/src/test/resources"
    ledger = json.loads((directory / "art-source-equivalence.json").read_text())
    if ledger.get("schema") != 1:
        raise ValueError("Unsupported original-art source equivalence")
    variants = {}
    for entry in ledger["proofs"][kind]:
        raw = (directory / entry["path"]).read_bytes()
        proof = json.loads(raw)
        if (core.digest(raw) != entry["sha256"] or
                proof.get("reference_payload_sha256", proof.get("reference_icons_sha256")) != payload_sha256):
            raise ValueError("Original-art equivalence does not match its actual producer/corpus")
        refs = proof.get("verified_refs", [proof.get("verified_ref")])
        if not refs or any(not isinstance(ref, str) or not re.fullmatch(r"[0-9a-f]{40}", ref) for ref in refs):
            raise ValueError("Original-art equivalence requires immutable source revisions")
        for path, values in proof["source_variants"].items():
            source_hashes = {core.digest(subprocess.run(["git", "show", ref + ":" + path], cwd=root,
                                                       capture_output=True, check=True).stdout) for ref in refs}
            allowed = source_hashes | {original_sources[path]}
            if not source_hashes.issubset(values) or set(values) - allowed:
                raise ValueError("Original-art source variant is not proved by its immutable source")
            variants.setdefault(path, set()).update(values)
    return variants


def artwork(root, main):
    sources = {}
    for fixture in ["ink-swift-reference.json.gz", "ink-icons-swift-reference.json.gz"]:
        directory = root / "android/app/src/test/resources"
        document = json.loads(gzip.decompress((directory / fixture).read_bytes()))
        payload = document["icons"] if fixture == "ink-icons-swift-reference.json.gz" else {
            key: value for key, value in document.items() if key != "sources"}
        variants = verified_art_variants(root, "icons" if "icons" in fixture else "ink",
                                         core.json_digest(payload), document["sources"])
        if fixture == "ink-icons-swift-reference.json.gz":
            equivalence = json.loads((directory / "ink-icons-source-equivalence.json").read_text())
            if core.json_digest(document["icons"]) != equivalence["reference_icons_sha256"]:
                raise ValueError("Swift source equivalence does not match the icon corpus")
            for path, values in equivalence["source_variants"].items():
                variants.setdefault(path, set()).update(values)
        for path, expected in document["sources"].items():
            accepted = set(variants.get(path, [expected]))
            if expected not in accepted:
                raise ValueError("Source equivalence must retain the original fixture: " + path)
            upstream = subprocess.run(["git", "show", main + ":" + path], cwd=root, capture_output=True, check=True).stdout
            hashes = {"main": core.digest(upstream), "source_snapshot": core.digest((root / path).read_bytes())}
            sources[path] = {"fixture_sha256": expected, "verified_equivalent_sha256": sorted(accepted - {expected}),
                             "input_sha256": hashes, "matches": all(value in accepted for value in hashes.values())}
    path = "apple/Shared/Apps/Native/PetViews.swift"
    expected = re.search(r'const val sourceSha256 = "([a-f0-9]{64})"',
                         (root / "android/app/src/main/java/xyz/tironi/zoen/ui/GeneratedPetArt.kt").read_text())[1]
    pet = (root / "android/app/src/main/java/xyz/tironi/zoen/ui/GeneratedPetArt.kt").read_text()
    pixels, count = re.subn(r'^    const val sourceSha256 = "[0-9a-f]{64}"\n', '', pet, flags=re.M)
    if count != 1:
        raise ValueError("Original pet provenance field is missing or duplicated")
    accepted = verified_art_variants(root, "pet", core.digest(pixels.encode()), {path: expected})[path]
    upstream = subprocess.run(["git", "show", main + ":" + path], cwd=root, capture_output=True, check=True).stdout
    hashes = {"main": core.digest(upstream), "source_snapshot": core.digest((root / path).read_bytes())}
    sources[path] = {"fixture_sha256": expected, "verified_equivalent_sha256": sorted(accepted - {expected}),
                     "input_sha256": hashes, "matches": all(value in accepted for value in hashes.values())}
    generated = subprocess.run(["python3", str(root / "scripts/export-android-pet.py"), "--check"],
                               cwd=root, capture_output=True, text=True)
    return {"original_sources": sources, "generated_pet_matches_original_source": generated.returncode == 0,
            "scope": "static original geometry/source equivalence; not renderer, device or motion acceptance"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch", action="store_true", help="Refresh only origin/main before verifying ancestry")
    parser.add_argument("--main-ref", default="origin/main", help="Explicit current Main input revision (resolved into the report)")
    parser.add_argument("--source-only", action="store_true", help="Pre-build source gate; does not certify generated artifacts")
    parser.add_argument("--abis", default="arm64-v8a,x86_64")
    parser.add_argument("--profile", choices=["dev", "release"])
    parser.add_argument("--output", type=Path, default=ROOT / "build/android-upstream.json")
    args = parser.parse_args()
    if args.fetch:
        core.git(ROOT, "fetch", "origin", "main")
    try:
        main_revision = core.git(ROOT, "rev-parse", args.main_ref + "^{commit}").stdout.strip()
        source = core.capture(ROOT)
        head = source["source"]["commit"]
        minimum = core.git(ROOT, "merge-base", "--is-ancestor", MINIMUM_MAIN, main_revision, check=False).returncode == 0
        ancestor = core.git(ROOT, "merge-base", "--is-ancestor", main_revision, head, check=False).returncode == 0
        comparison = compare_exports(ROOT, main_revision, source)
        main_delta = comparison["raw_abi_delta"]
        art = artwork(ROOT, main_revision)
        apple_inputs = {path.relative_to(ROOT).as_posix(): core.digest(path.read_bytes())
                        for path in sorted((ROOT / "apple/Shared").rglob("*"))
                        if path.is_file() and path.suffix in {".swift", ".xcstrings"}}
        errors = []
        if not minimum:
            errors.append("Main reference predates the merged PR71 minimum")
        if not ancestor:
            errors.append("Current Main is not an ancestor of the Android source revision")
        if not all(value["matches"] for value in art["original_sources"].values()) or not art["generated_pet_matches_original_source"]:
            errors.append("Current original art needs regenerated or independently verified equivalence fixtures")
        generated = {"status": "not-checked-pre-build", "runtime_validation": "not established by this source gate"}
        if not args.source_only:
            try:
                generated = core.verify(ROOT, core.OUTPUT, args.abis.split(","), args.profile)
            except (OSError, ValueError, KeyError) as error:
                generated = {"status": "failed", "error": str(error), "runtime_validation": "not established"}
                errors.append(str(error))
        report = {"checked_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  "main_ref": args.main_ref, "main_revision": main_revision, "minimum_main_revision": MINIMUM_MAIN,
                  "minimum_main_is_ancestor": minimum, "latest_main_is_ancestor": ancestor,
                  "source_snapshot": source, "exported_api_delta_from_main": main_delta,
                  "source_contract_comparison": comparison,
                  "apple_source_snapshot": {"revision": source["source"], "inputs": apple_inputs,
                                            "input_sha256": core.json_digest(apple_inputs),
                                            "changed_paths_from_main": core.git(ROOT, "diff", "--name-only", main_revision, "--", "apple/Shared").stdout.splitlines(),
                                            "runtime_validation": "current UI requires its own device journeys and visual evidence"},
                  "original_art": art, "generated_core": generated, "errors": errors}
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps({"report": str(args.output), "source": source["source"], "main_revision": main_revision,
                          "input_sha256": source["input_sha256"], "generated_core": generated["status"],
                          "art_sources_match": all(value["matches"] for value in art["original_sources"].values()), "errors": errors}, indent=2))
        if errors:
            raise SystemExit("Android source/ABI gate failed: " + "; ".join(errors))
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error


if __name__ == "__main__":
    main()
