import hashlib
import fcntl
import json
import os
import pathlib
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest


GUARD = pathlib.Path(__file__).resolve().parents[1] / "cargo-source-guard.py"
CARGO = os.environ.get("CARGO_TEST_BIN") or shutil.which("cargo")


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


class Fixture:
    def __init__(self, test):
        parent = os.environ.get("CARGO_SOURCE_GUARD_TEST_ROOT")
        if parent:
            pathlib.Path(parent).mkdir(parents=True, exist_ok=True)
        self.root = pathlib.Path(tempfile.mkdtemp(prefix=test + "-", dir=parent))
        self.target = self.root / "target"
        self.vendor = self.root / "vendor" / "guard-dep"
        write(self.vendor / "Cargo.toml", '[package]\nname="guard-dep"\nversion="0.1.0"\nedition="2021"\n')
        write(self.vendor / "src/lib.rs", "pub fn marker() -> u8 { 7 }\n")
        files = {name: hashlib.sha256((self.vendor / name).read_bytes()).hexdigest() for name in ("Cargo.toml", "src/lib.rs")}
        write(self.vendor / ".cargo-checksum.json", json.dumps({"package": "0" * 64, "files": files}))
        self.a, self.b = self.root / "a", self.root / "b"
        for directory, marker in ((self.a, "A"), (self.b, "B")):
            write(directory / "Cargo.toml", '[package]\nname="source-probe"\nversion="0.1.0"\nedition="2021"\n[dependencies]\nguard-dep="=0.1.0"\n')
            write(directory / "src/main.rs", 'fn main() { println!("' + marker + '|{}|{}", env!("CARGO_MANIFEST_DIR"), guard_dep::marker()); }\n')
            write(directory / ".cargo/config.toml", '[source.crates-io]\nreplace-with="fixture-vendor"\n[source.fixture-vendor]\ndirectory=' + json.dumps(str(self.vendor.parent)) + '\n')
            write(directory / ".gitignore", "/target/\n")
            self.raw(directory, ["generate-lockfile", "--offline"])
            subprocess.run(["git", "init", "-q", str(directory)], check=True)
            subprocess.run(["git", "add", "."], cwd=directory, check=True)
            for path in directory.rglob("*"):
                if path.is_file() and ".git" not in path.relative_to(directory).parts:
                    os.utime(path, (1_000_000_000, 1_000_000_000))
        self.serial = 0

    def env(self, extra=None):
        env = os.environ.copy()
        env["CARGO_TARGET_DIR"] = str(self.target)
        env["PATH"] = str(pathlib.Path(CARGO).parent) + os.pathsep + env.get("PATH", "")
        env["CARGO_BUILD_JOBS"] = "1"
        env.update(extra or {})
        return env

    def raw(self, directory, args):
        result = subprocess.run([CARGO, *args], cwd=directory, env=self.env(), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=30)
        if result.returncode:
            raise AssertionError(result.stdout.decode())
        return result.stdout.decode()

    def argv(self, directory, extra=(), command=("build",)):
        self.serial += 1
        receipt = self.root / ("receipt-" + str(self.serial) + ".json")
        argv = [sys.executable, str(GUARD), "--cargo", CARGO, "--manifest-path", str(directory / "Cargo.toml"), "--target-dir", str(self.target), "--receipt", str(receipt), *extra, "--", *command]
        return argv, receipt

    def guarded(self, directory, extra=(), command=("build",), env=None):
        argv, receipt = self.argv(directory, extra, command)
        result = subprocess.run(argv, env=self.env(env), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=40)
        log = self.root / (receipt.stem + ".log")
        log.write_bytes(result.stdout)
        return result, json.loads(receipt.read_text()) if receipt.exists() else None

    def output(self):
        return subprocess.check_output([str(self.target / "debug/source-probe")], text=True).strip()

    def dependency(self):
        paths = list((self.target / "debug/deps").glob("libguard_dep-*.rlib"))
        if len(paths) != 1:
            raise AssertionError(paths)
        return paths[0], paths[0].stat().st_mtime_ns, hashlib.sha256(paths[0].read_bytes()).hexdigest()

    def blocker(self, directory):
        gate, marker = self.root / "gate", self.root / "started"
        gate.write_text("hold")
        write(directory / "build.rs", 'fn main() { if let Ok(gate)=std::env::var("PROBE_GATE") { std::fs::write(std::env::var("PROBE_MARKER").unwrap(), "started").unwrap(); while std::path::Path::new(&gate).exists() { std::thread::sleep(std::time::Duration::from_millis(10)); } } }\n')
        subprocess.run(["git", "add", "build.rs"], cwd=directory, check=True)
        argv, receipt = self.argv(directory)
        stream = (self.root / "blocked.log").open("wb")
        child = subprocess.Popen(argv, env=self.env({"PROBE_GATE": str(gate), "PROBE_MARKER": str(marker)}), stdout=stream, stderr=subprocess.STDOUT)
        deadline = time.monotonic() + 20
        while not marker.exists() and child.poll() is None and time.monotonic() < deadline:
            time.sleep(0.02)
        if not marker.exists():
            child.terminate()
            child.wait(timeout=8)
            stream.close()
            raise AssertionError((self.root / "blocked.log").read_text())
        return child, stream, receipt, gate


class CargoSourceGuardTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not CARGO:
            raise RuntimeError("Set CARGO_TEST_BIN to an installed Cargo executable")

    def fixture(self):
        fixture = Fixture(self._testMethodName)
        if not os.environ.get("CARGO_SOURCE_GUARD_TEST_ROOT"):
            self.addCleanup(shutil.rmtree, fixture.root)
        return fixture

    def test_real_old_mtime_reproducer_and_guarded_reuse(self):
        f = self.fixture()
        f.raw(f.a, ["build", "--offline", "--locked"])
        self.assertEqual(f.output(), "A|" + str(f.a) + "|7")
        dependency = f.dependency()
        raw = f.raw(f.b, ["build", "--offline", "--locked", "-v"])
        raw_output = f.output()
        (f.root / "unprotected-observation.json").write_text(json.dumps({"output": raw_output, "staleA": raw_output.startswith("A|"), "cargo": f.raw(f.b, ["-Vv"]), "log": raw}, indent=2))
        self.assertIn(raw_output, ("A|" + str(f.a) + "|7", "B|" + str(f.b) + "|7"))
        refused, receipt = f.guarded(f.b)
        self.assertNotEqual(refused.returncode, 0)
        self.assertIn("Unmanaged nonempty target", receipt["error"])
        fixed, receipt = f.guarded(f.b, ("--adopt-idle-target",))
        self.assertEqual(fixed.returncode, 0, fixed.stdout.decode())
        self.assertEqual(f.output(), "B|" + str(f.b) + "|7")
        self.assertEqual(len(receipt["cleanedPackages"]), 1)
        self.assertTrue(all(not value["fresh"] for value in receipt["artifacts"]))
        self.assertEqual(f.dependency(), dependency)
        fresh, receipt = f.guarded(f.b)
        self.assertEqual(fresh.returncode, 0, fresh.stdout.decode())
        self.assertEqual(receipt["cleanedPackages"], [])
        self.assertTrue(all(value["fresh"] for value in receipt["artifacts"]))
        for source, marker in ((f.a, "A"), (f.b, "B")):
            result, receipt = f.guarded(source)
            self.assertEqual(result.returncode, 0, result.stdout.decode())
            self.assertEqual(len(receipt["cleanedPackages"]), 1)
            self.assertEqual(f.output(), marker + "|" + str(source) + "|7")
            self.assertEqual(f.dependency(), dependency)

    def test_same_owner_changed_bytes_with_old_mtime_rebuild(self):
        f = self.fixture()
        result, _ = f.guarded(f.b)
        self.assertEqual(result.returncode, 0, result.stdout.decode())
        path = f.b / "src/main.rs"
        path.write_text(path.read_text().replace('"B|', '"C|'))
        os.utime(path, (1_000_000_000, 1_000_000_000))
        result, receipt = f.guarded(f.b)
        self.assertEqual(result.returncode, 0, result.stdout.decode())
        self.assertEqual(len(receipt["cleanedPackages"]), 1)
        self.assertEqual(f.output(), "C|" + str(f.b) + "|7")

    def test_source_change_during_build_leaves_unqualified_stamp(self):
        f = self.fixture()
        write(f.a / "watched.txt", "before")
        write(f.a / "build.rs", 'fn main() { std::fs::write("watched.txt", "after").unwrap(); }\n')
        subprocess.run(["git", "add", "."], cwd=f.a, check=True)
        result, receipt = f.guarded(f.a)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("changed during Cargo", receipt["error"])
        state = json.loads(next(f.root.glob(".cargo-source-guard-*/state.json")).read_text())
        self.assertEqual(state["status"], "in_progress")

    def test_concurrent_guard_refuses_without_cleaning_and_signal_recovers(self):
        f = self.fixture()
        child, stream, receipt, gate = f.blocker(f.a)
        try:
            second, value = f.guarded(f.b)
            self.assertNotEqual(second.returncode, 0)
            self.assertIn("busy", value["error"])
            self.assertNotIn("cleanedPackages", value)
            child.terminate()
            self.assertNotEqual(child.wait(timeout=10), 0)
            self.assertEqual(json.loads(receipt.read_text())["status"], "FAIL")
        finally:
            gate.unlink(missing_ok=True)
            if child.poll() is None:
                child.kill()
                child.wait()
            stream.close()
        result, value = f.guarded(f.b)
        self.assertEqual(result.returncode, 0, result.stdout.decode())
        self.assertEqual(len(value["cleanedPackages"]), 1)
        self.assertEqual(f.output(), "B|" + str(f.b) + "|7")

    def test_killed_guard_keeps_lease_until_its_actual_cargo_exits(self):
        f = self.fixture()
        child, stream, _, gate = f.blocker(f.a)
        state_path = next(f.root.glob(".cargo-source-guard-*/state.json"))
        cargo_pid = json.loads(state_path.read_text())["cargoPid"]
        try:
            child.kill()
            child.wait(timeout=5)
            second, receipt = f.guarded(f.b)
            self.assertNotEqual(second.returncode, 0)
            self.assertIn("busy", receipt["error"])
        finally:
            gate.unlink(missing_ok=True)
            try:
                os.killpg(cargo_pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            stream.close()
        lease = next(f.root.glob(".cargo-source-guard-*/lease"))
        with lease.open("rb") as lock:
            deadline = time.monotonic() + 5
            while True:
                try:
                    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    fcntl.flock(lock, fcntl.LOCK_UN)
                    break
                except BlockingIOError:
                    if time.monotonic() >= deadline:
                        self.fail("owned Cargo lease did not release after process-group termination")
                    time.sleep(0.02)
        result, value = f.guarded(f.b)
        self.assertEqual(result.returncode, 0, result.stdout.decode())
        self.assertEqual(len(value["cleanedPackages"]), 1)

    def test_target_profile_namespaces_do_not_accept_foreign_fresh(self):
        f = self.fixture()
        host = next(line.split(": ", 1)[1] for line in f.raw(f.a, ["-Vv"]).splitlines() if line.startswith("host: "))
        for directory in (f.a, f.b):
            with (directory / "Cargo.toml").open("a") as stream:
                stream.write('\n[profile.probe]\ninherits="dev"\n')
            write(directory / "build.rs", 'fn main() { println!("cargo:rustc-env=PROBE_BUILD_SOURCE={}", env!("CARGO_MANIFEST_DIR")); }\n')
            subprocess.run(["git", "add", "."], cwd=directory, check=True)
            for name in ("Cargo.toml", "build.rs"):
                os.utime(directory / name, (1_000_000_000, 1_000_000_000))
        for flags in ([], ["--target", host], ["--release"], ["--profile", "probe"]):
            f.raw(f.a, ["build", "--offline", "--locked", *flags])
        initial_release = hashlib.sha256((f.target / "release/source-probe").read_bytes()).hexdigest()
        result, _ = f.guarded(f.b, ("--adopt-idle-target",))
        self.assertEqual(result.returncode, 0, result.stdout.decode())
        default_binary = (f.target / "debug/source-probe").read_bytes()
        for flags, relative in ((["--target", host], host + "/debug/source-probe"), (["--release"], "release/source-probe"), (["--profile", "probe"], "probe/source-probe")):
            result, value = f.guarded(f.b, command=("build", *flags))
            self.assertEqual(result.returncode, 0, result.stdout.decode())
            self.assertEqual(len(value["cleanedPackages"]), 1)
            output = subprocess.check_output([str(f.target / relative)], text=True).strip()
            self.assertEqual(output, "B|" + str(f.b) + "|7")
            self.assertEqual((f.target / "debug/source-probe").read_bytes(), default_binary)
            if flags[:1] == ["--target"]:
                self.assertEqual(hashlib.sha256((f.target / "release/source-probe").read_bytes()).hexdigest(), initial_release)
        result, value = f.guarded(f.b)
        self.assertEqual(result.returncode, 0, result.stdout.decode())
        self.assertEqual(value["cleanedPackages"], [])
        # Cargo may legitimately rerun build.rs after a target/profile change.
        # The guard must not clean again or accept a foreign manifest/output.
        self.assertTrue(all(item["manifest"] == str(f.b / "Cargo.toml") for item in value["artifacts"]))
        self.assertEqual(f.output(), "B|" + str(f.b) + "|7")

    def test_local_registry_name_collision_refuses_before_clean(self):
        f = self.fixture()
        f.raw(f.a, ["build", "--offline", "--locked"])
        dependency = f.dependency()
        manifest = f.b / "Cargo.toml"
        manifest.write_text(manifest.read_text().replace('name="source-probe"', 'name="guard-dep"').replace('version="0.1.0"', 'version="0.2.0"'))
        f.raw(f.b, ["generate-lockfile", "--offline"])
        result, value = f.guarded(f.b, ("--adopt-idle-target",))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("name collision", value["error"])
        self.assertNotIn("cleanedPackages", value)
        self.assertEqual(f.dependency(), dependency)

    def test_effective_target_and_build_directory_are_owned(self):
        f = self.fixture()
        host = next(line.split(": ", 1)[1] for line in f.raw(f.a, ["-Vv"]).splitlines() if line.startswith("host: "))
        foreign = f.root / "foreign"
        with (f.a / ".cargo/config.toml").open("a") as stream:
            stream.write('\n[build]\ntarget=' + json.dumps(host) + '\nbuild-dir=' + json.dumps(str(foreign)) + '\n')
        result, value = f.guarded(f.a, env={"CARGO_TARGET_DIR": str(foreign), "CARGO_BUILD_BUILD_DIR": str(foreign)})
        self.assertEqual(result.returncode, 0, result.stdout.decode())
        self.assertEqual(subprocess.check_output([str(f.target / host / "debug/source-probe")], text=True).strip(), "A|" + str(f.a) + "|7")
        self.assertFalse(foreign.exists())
        self.assertEqual(value["scope"], {"targets": [host], "profile": "dev"})
        result, value = f.guarded(f.a, env={"CARGO_BUILD_TARGET": host})
        self.assertEqual(result.returncode, 0, result.stdout.decode())
        self.assertEqual(value["scope"]["targets"], [host])

    def test_previously_recorded_registry_name_is_not_cleaned_as_local(self):
        f = self.fixture()
        result, _ = f.guarded(f.a)
        self.assertEqual(result.returncode, 0, result.stdout.decode())
        dependency = f.dependency()
        write(f.b / "Cargo.toml", '[package]\nname="guard-dep"\nversion="0.2.0"\nedition="2021"\n')
        write(f.b / "src/main.rs", 'fn main() {}\n')
        f.raw(f.b, ["generate-lockfile", "--offline"])
        result, value = f.guarded(f.b)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("third-party cache history", value["error"])
        self.assertNotIn("cleanedPackages", value)
        self.assertEqual(f.dependency(), dependency)

    def test_config_and_env_are_bound_without_exposing_values(self):
        f = self.fixture()
        secret = "synthetic-build-input-do-not-log"
        result, first = f.guarded(f.a, ("--input-env", "PROBE_INPUT"), env={"PROBE_INPUT": secret})
        self.assertEqual(result.returncode, 0, result.stdout.decode())
        result, second = f.guarded(f.a, ("--input-env", "PROBE_INPUT"), env={"PROBE_INPUT": secret + "-changed"})
        self.assertEqual(result.returncode, 0, result.stdout.decode())
        self.assertNotEqual(first["before"]["contextDigest"], second["before"]["contextDigest"])
        self.assertEqual(len(second["cleanedPackages"]), 1)
        self.assertNotIn(secret, json.dumps(first))
        self.assertNotIn(secret, json.dumps(second))

    def test_cargo_overrides_and_unsafe_sidecar_are_rejected(self):
        f = self.fixture()
        for flag in ("--manifest-path=" + str(f.a / "Cargo.toml"), "--target-dir=/tmp/foreign-target", "--config=build.target-dir='foreign'"):
            result, _ = f.guarded(f.a, command=("build", flag))
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("overrides are owned", result.stdout.decode())
        target_digest = hashlib.sha256(json.dumps(str(f.target), sort_keys=True, separators=(",", ":")).encode()).hexdigest()[:24]
        sidecar = f.root / (".cargo-source-guard-" + target_digest)
        sidecar.symlink_to(f.a, target_is_directory=True)
        result, receipt = f.guarded(f.a)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("private directory", receipt["error"])


if __name__ == "__main__":
    unittest.main()
