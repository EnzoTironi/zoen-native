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


def artwork(root, main):
    sources = {}
    accepted_sources = {}
    for fixture, equivalence_file in [("ink-swift-reference.json.gz", "ink-source-equivalence.json"),
                                      ("ink-icons-swift-reference.json.gz", "ink-icons-source-equivalence.json")]:
        directory = root / "android/app/src/test/resources"
        document = json.loads(gzip.decompress((directory / fixture).read_bytes()))
        variants = {}
        equivalence_path = directory / equivalence_file
        if equivalence_path.exists() or fixture == "ink-icons-swift-reference.json.gz":
            equivalence = json.loads(equivalence_path.read_text())
            if fixture == "ink-icons-swift-reference.json.gz":
                corpus, expected = document["icons"], equivalence["reference_icons_sha256"]
            else:
                corpus = {key: value for key, value in document.items() if key != "sources"}
                expected = equivalence["reference_corpus_sha256"]
            if core.json_digest(corpus) != expected:
                raise ValueError("Swift source equivalence does not match the corpus: " + fixture)
            variants = equivalence["source_variants"]
        for path, expected in document["sources"].items():
            accepted = set(variants.get(path, [expected]))
            if expected not in accepted:
                raise ValueError("Source equivalence must retain the original fixture: " + path)
            # Shared inputs must be verified by every dependent corpus, not just the last one.
            if path in accepted_sources:
                if sources[path]["fixture_sha256"] != expected:
                    raise ValueError("Art corpora disagree about the original source: " + path)
                accepted &= accepted_sources[path]
            accepted_sources[path] = accepted
            upstream = subprocess.run(["git", "show", main + ":" + path], cwd=root, capture_output=True, check=True).stdout
            hashes = {"main": core.digest(upstream), "source_snapshot": core.digest((root / path).read_bytes())}
            sources[path] = {"fixture_sha256": expected, "verified_equivalent_sha256": sorted(accepted - {expected}),
                             "input_sha256": hashes, "matches": all(value in accepted for value in hashes.values())}
    path = "apple/Shared/Apps/Native/PetViews.swift"
    expected = re.search(r'const val sourceSha256 = "([a-f0-9]{64})"',
                         (root / "android/app/src/main/java/xyz/tironi/zoen/ui/GeneratedPetArt.kt").read_text())[1]
    upstream = subprocess.run(["git", "show", main + ":" + path], cwd=root, capture_output=True, check=True).stdout
    hashes = {"main": core.digest(upstream), "source_snapshot": core.digest((root / path).read_bytes())}
    sources[path] = {"fixture_sha256": expected, "input_sha256": hashes, "matches": all(value == expected for value in hashes.values())}
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
        baseline = core.contract_at(ROOT, main_revision)
        current = source["contract"]
        main_delta = {"added": sorted(current["metadata"].keys() - baseline["metadata"].keys()),
                      "removed": sorted(baseline["metadata"].keys() - current["metadata"].keys()),
                      "changed": sorted(name for name, shape in baseline["metadata"].items()
                                        if name in current["metadata"] and current["metadata"][name] != shape)}
        art = artwork(ROOT, main_revision)
        apple_inputs = {path.relative_to(ROOT).as_posix(): core.digest(path.read_bytes())
                        for path in sorted((ROOT / "apple/Shared").rglob("*"))
                        if path.is_file() and path.suffix in {".swift", ".xcstrings"}}
        errors = []
        if not minimum:
            errors.append("Main reference predates the merged PR71 minimum")
        if not ancestor:
            errors.append("Current Main is not an ancestor of the Android source revision")
        if main_delta["removed"]:
            errors.append("Current source removes exported Main APIs: " + ", ".join(main_delta["removed"]))
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
