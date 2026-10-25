"""Real Cargo host/simulator/profile reuse; never uses the application workspace."""
import json
import os
import shutil
import subprocess
import sys
import unittest

from test_cargo_source_guard import CARGO, Fixture, write


class CargoGuardScopeReuseTests(unittest.TestCase):
    def test_host_release_simulator_bindgen_host_release_and_changed_inputs(self):
        f = Fixture(self._testMethodName)
        if not os.environ.get("CARGO_SOURCE_GUARD_TEST_ROOT"):
            self.addCleanup(shutil.rmtree, f.root)
        host = next(line.split(": ", 1)[1] for line in f.raw(f.a, ["-Vv"]).splitlines()
                    if line.startswith("host: "))
        simulator = os.environ.get("CARGO_GUARD_SIMULATOR_TARGET", host)
        apple_simulator = simulator.endswith("-apple-ios-sim")
        # Portable CI uses the real explicit-host namespace. The Apple opt-in
        # uses the genuine installed simulator target and public Xcode SDK.
        if sys.platform == "darwin":
            host_sdk = subprocess.check_output(["xcrun", "--sdk", "macosx", "--show-sdk-path"], text=True).strip()
        else:
            host_sdk = "host-sdk-fixture"
        sim_sdk = (subprocess.check_output(["xcrun", "--sdk", "iphonesimulator", "--show-sdk-path"], text=True).strip()
                   if apple_simulator else host_sdk + "//")
        manifest = f.a / "Cargo.toml"
        manifest.write_text(manifest.read_text() + '\n[[bin]]\nname="guard-bindgen"\npath="src/bindgen.rs"\n')
        write(f.a / "src/bindgen.rs", 'fn main() { println!("fixture binding generator"); }\n')
        source = f.a / "src/main.rs"
        source.write_text('fn main() { println!("A|{}|{}|{}", env!("CARGO_MANIFEST_DIR"), guard_dep::marker(), env!("SDKROOT")); }\n')
        subprocess.run(["git", "add", "."], cwd=f.a, check=True)
        release = ("build", "--release", "--bin", "source-probe")
        bindgen = ("build", "--bin", "guard-bindgen")
        sim = (*release, "--target", simulator)
        declared = ("--input-env", "SDKROOT")
        host_env = {"SDKROOT": host_sdk}
        sim_env = {"SDKROOT": sim_sdk, "IPHONEOS_DEPLOYMENT_TARGET": "26.0"}
        records = []

        def build(command, env):
            result, receipt = f.guarded(f.a, declared, command=command, env=env)
            self.assertEqual(result.returncode, 0, result.stdout.decode())
            records.append(receipt)
            return receipt

        sequence = [(release, host_env), (sim, sim_env), (bindgen, host_env), (release, host_env)]
        # First qualification may clean unknown scopes. Cargo's cross-target
        # clean can also remove host fingerprints, so warm the complete pipeline
        # before measuring the next identical cohort. No fake Cargo or output.
        for command, env in sequence:
            build(command, env)
        measured = [build(command, env) for command, env in sequence]
        for receipt in measured:
            self.assertEqual(receipt["cleanedPackages"], [])
            self.assertTrue(all(a["fresh"] for a in receipt["artifacts"]))
        reused = measured[-1]
        self.assertEqual(measured[0]["before"], reused["before"])
        state_path = next(f.root.glob(".cargo-source-guard-*/state.json"))
        self.assertEqual(len(json.loads(state_path.read_text())["scopes"]), 3)
        binary = f.target / "release/source-probe"
        self.assertEqual(subprocess.check_output([str(binary)], text=True).strip(),
                         "A|" + str(f.a) + "|7|" + host_sdk)
        # A trailing slash is still the real SDK on macOS. Its changed declared
        # locator must clean/recompile this scope rather than trust old bytes.
        changed_sdk = build(release, {"SDKROOT": host_sdk + "/"})
        self.assertEqual(len(changed_sdk["cleanedPackages"]), 1)
        self.assertTrue(all(not a["fresh"] for a in changed_sdk["artifacts"]))
        self.assertNotEqual(reused["before"]["contextDigest"], changed_sdk["before"]["contextDigest"])
        self.assertTrue(subprocess.check_output([str(binary)], text=True).strip().endswith(host_sdk + "/"))
        # The SDK change belongs to release/host, not the disjoint dev generator.
        generator = build(bindgen, host_env)
        self.assertEqual(generator["cleanedPackages"], [])
        self.assertTrue(all(a["fresh"] for a in generator["artifacts"]))
        restored_sdk = build(release, host_env)
        self.assertEqual(len(restored_sdk["cleanedPackages"]), 1)
        source.write_text(source.read_text().replace('"A|', '"C|'))
        os.utime(source, (1_000_000_000, 1_000_000_000))
        changed_source = build(release, host_env)
        self.assertEqual(len(changed_source["cleanedPackages"]), 1)
        self.assertTrue(subprocess.check_output([str(binary)], text=True).strip().startswith("C|"))
        self.assertEqual(len(json.loads(state_path.read_text())["scopes"]), 1)
        self.assertEqual(len(build(bindgen, host_env)["cleanedPackages"]), 1)
        (f.root / "sequence.json").write_text(json.dumps({"simulatorTarget": simulator,
            "realAppleSimulator": apple_simulator, "receipts": len(records)}, indent=2))


if __name__ == "__main__":
    unittest.main()
