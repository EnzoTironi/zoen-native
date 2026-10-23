from pathlib import Path
import hashlib
import importlib.util
import json
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("pinned_chrome_install", Path(__file__).with_name("install.py"))
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class ArtifactAdmission(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zoen-cft-pin-test-")
        self.addCleanup(self.temp.cleanup)
        self.folder = Path(self.temp.name)

    def pin(self):
        return {"version": "156.0.8078.47", "revision": "1706539", "platform": "linux64", "url": "https://storage.googleapis.com/chrome-for-testing-public/156.0.8078.47/linux64/chrome-linux64.zip", "bytes": 5, "sha256": "ab" * 32}

    def archive(self, entries):
        path = self.folder / "browser.zip"
        with zipfile.ZipFile(path, "w") as output:
            for name, body in entries:
                output.writestr(name, body)
        return path, {"bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}

    def test_pin_is_exact_version_url_digest_and_length_not_a_channel(self):
        path = self.folder / "pin.json"
        pin = self.pin()
        path.write_text(json.dumps(pin))
        self.assertEqual(installer.read_pin(path), pin)
        for field, value in [("version", "stable"), ("platform", "mac-arm64"), ("url", "https://foreign.example/browser.zip"), ("sha256", "bad"), ("bytes", -1), ("bytes", True), ("bytes", 301 * 1024 * 1024)]:
            with self.subTest(field=field, value=value):
                changed = {**pin, field: value}
                path.write_text(json.dumps(changed))
                with self.assertRaises(ValueError):
                    installer.read_pin(path)

    def test_archive_requires_exact_bytes_and_digest(self):
        path, pin = self.archive([("chrome-linux64/chrome", b"synthetic")])
        installer.verify_archive(path, pin)
        for changed in [{**pin, "bytes": pin["bytes"] + 1}, {**pin, "sha256": "a" * 64}]:
            with self.assertRaises(ValueError):
                installer.verify_archive(path, changed)

    def test_foreign_traversal_duplicate_symlink_and_missing_executable_refuse(self):
        for name in ["../chrome", "/chrome-linux64/chrome", "chrome-linux64/../../foreign", "foreign/chrome", "chrome-linux64\\foreign", "chrome-linux64/./chrome", "chrome-linux64//chrome"]:
            with self.subTest(name=name):
                path, pin = self.archive([("chrome-linux64/chrome", b"synthetic"), (name, b"synthetic")])
                with self.assertRaises(ValueError):
                    installer.verify_archive(path, pin)
        with self.assertWarns(UserWarning):
            path, pin = self.archive([("chrome-linux64/chrome", b"a"), ("chrome-linux64/chrome", b"b")])
        with self.assertRaises(ValueError):
            installer.verify_archive(path, pin)
        symlink = zipfile.ZipInfo("chrome-linux64/chrome")
        symlink.create_system = 3
        symlink.external_attr = 0o120777 << 16
        path, pin = self.archive([(symlink, b"/foreign/browser")])
        with self.assertRaises(ValueError):
            installer.verify_archive(path, pin)
        path, pin = self.archive([("chrome-linux64/other", b"synthetic")])
        with self.assertRaises(ValueError):
            installer.verify_archive(path, pin)

    def test_namespace_policy_is_scoped_to_one_pinned_immutable_executable(self):
        path = "/usr/lib/zoen-cft/156.0.8078.47-abababababab/chrome-linux64/chrome"
        policy = installer.namespace_policy(Path(path))
        self.assertIn('"' + path + '"', policy)
        self.assertIn("userns,", policy)
        self.assertNotIn("*", policy)
        self.assertNotIn("sysctl", policy)
        for bad in ["/tmp/chrome", path.replace("/usr/lib/", "/opt/"), path + "*", path.replace("abababababab", "../outside")]:
            with self.assertRaises(ValueError):
                installer.namespace_policy(Path(bad))

    def test_mutable_and_symlink_installation_parents_are_not_repaired(self):
        mutable = self.folder / "mutable"
        mutable.mkdir()
        mutable.chmod(0o720)
        with self.assertRaises(RuntimeError):
            installer.require_protected_directory(mutable)
        self.assertEqual(mutable.stat().st_mode & 0o777, 0o720)
        link = self.folder / "link"
        link.symlink_to(mutable, target_is_directory=True)
        with self.assertRaises(RuntimeError):
            installer.require_protected_directory(link)
        self.assertTrue(link.is_symlink())


if __name__ == "__main__":
    unittest.main()
