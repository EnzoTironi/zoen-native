"""Real Cargo keeps the guard conservative when vendored/registry names overlap."""
import subprocess
import unittest
from test_cargo_source_guard import CARGO, Fixture


class CollateralCleanupTests(unittest.TestCase):
    def test_explicit_collision_rebuilds_and_records_both_identities(self):
        if not CARGO:
            self.fail("Set CARGO_TEST_BIN to an installed Cargo executable")
        fixture = Fixture(self._testMethodName)
        self.addCleanup(__import__("shutil").rmtree, fixture.root)
        fixture.raw(fixture.a, ["build", "--offline", "--locked"])
        old_dependency = fixture.dependency()
        manifest = fixture.b / "Cargo.toml"
        manifest.write_text(manifest.read_text().replace('name="source-probe"', 'name="guard-dep"').replace('version="0.1.0"', 'version="0.2.0"'))
        fixture.raw(fixture.b, ["generate-lockfile", "--offline"])
        refused, receipt = fixture.guarded(fixture.b, ("--adopt-idle-target",))
        self.assertNotEqual(refused.returncode, 0)
        self.assertIn("name collision", receipt["error"])
        self.assertEqual(fixture.dependency(), old_dependency)
        option = ("--adopt-idle-target", "--allow-collateral-package-clean", "guard-dep")
        passed, receipt = fixture.guarded(fixture.b, option)
        self.assertEqual(passed.returncode, 0, passed.stdout.decode())
        self.assertEqual(receipt["status"], "PASS")
        self.assertEqual(receipt["before"], receipt["after"])
        self.assertEqual(len(receipt["collateralCleanedPackageIds"]), 1)
        self.assertIn("registry+", receipt["collateralCleanedPackageIds"][0])
        self.assertTrue(all("path+file:" in identity for identity in receipt["cleanedPackages"]))
        self.assertTrue(all(not artifact["fresh"] for artifact in receipt["artifacts"]))
        self.assertNotEqual(fixture.dependency()[1], old_dependency[1], "Cargo must recompile the collateral registry dependency")
        output = subprocess.check_output([str(fixture.target / "debug/guard-dep")], text=True).strip()
        self.assertEqual(output, "B|" + str(fixture.b.resolve()) + "|7")
        passed, fresh = fixture.guarded(fixture.b, option)
        self.assertEqual(passed.returncode, 0, passed.stdout.decode())
        self.assertEqual(fresh["cleanedPackages"], [])
        self.assertEqual(fresh["collateralCleanedPackageIds"], [])
        refused, unknown = fixture.guarded(fixture.b, (*option, "--allow-collateral-package-clean", "unrelated"))
        self.assertNotEqual(refused.returncode, 0)
        self.assertNotIn("cleanedPackages", unknown)


    def test_explicit_history_only_collision_rebuilds_local_replacement(self):
        fixture = Fixture(self._testMethodName)
        self.addCleanup(__import__("shutil").rmtree, fixture.root)
        first, bound = fixture.guarded(fixture.a)
        self.assertEqual(first.returncode, 0, first.stdout.decode())
        self.assertIn("guard-dep", bound["before"]["thirdPartyNames"])
        manifest = fixture.b / "Cargo.toml"
        text = manifest.read_text().replace('name="source-probe"', 'name="guard-dep"')
        manifest.write_text(text.split("[dependencies]")[0])
        (fixture.b / "src/main.rs").write_text('fn main(){println!("local replacement");}')
        fixture.raw(fixture.b, ["generate-lockfile", "--offline"])
        refused, receipt = fixture.guarded(fixture.b)
        self.assertNotEqual(refused.returncode, 0)
        self.assertIn("name collision", receipt["error"])
        passed, receipt = fixture.guarded(fixture.b, ("--allow-collateral-package-clean", "guard-dep"))
        self.assertEqual(passed.returncode, 0, passed.stdout.decode())
        self.assertEqual(receipt["historicalCollateralPackageNames"], ["guard-dep"])
        self.assertEqual(receipt["collateralCleanedPackageIds"], [])
        self.assertTrue(all(not artifact["fresh"] for artifact in receipt["artifacts"]))
        output = subprocess.check_output([str(fixture.target / "debug/guard-dep")], text=True).strip()
        self.assertEqual(output, "local replacement")


if __name__ == "__main__":
    unittest.main()
