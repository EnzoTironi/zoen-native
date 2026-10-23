"""The Google-published CfT artifact, with repository-pinned bytes and no npm installer.

https://developer.chrome.com/docs/automation-and-testing/download-test-binaries
https://chromium.googlesource.com/chromium/src/+/main/docs/security/apparmor-userns-restrictions.md
"""
from pathlib import Path, PurePosixPath
import hashlib
import json
import os
import platform
import re
import stat
import subprocess
import tempfile
import zipfile


def read_pin(path):
    pin = json.loads(path.read_text())
    if set(pin) != {"version", "revision", "platform", "url", "bytes", "sha256"}:
        raise ValueError("Incomplete browser artifact pin")
    if not re.fullmatch(r"\d+\.\d+\.\d+\.\d+", pin["version"]):
        raise ValueError("Invalid browser version")
    if not re.fullmatch(r"\d+", pin["revision"]) or pin["platform"] != "linux64":
        raise ValueError("Unsupported browser artifact")
    expected = f'https://storage.googleapis.com/chrome-for-testing-public/{pin["version"]}/linux64/chrome-linux64.zip'
    if pin["url"] != expected or not re.fullmatch(r"[0-9a-f]{64}", pin["sha256"]):
        raise ValueError("Invalid browser artifact identity")
    if type(pin["bytes"]) is not int or not 0 < pin["bytes"] <= 300 * 1024 * 1024:
        raise ValueError("Invalid browser artifact size")
    return pin


def verify_archive(path, pin):
    if path.stat().st_size != pin["bytes"]:
        raise ValueError("Browser artifact length mismatch")
    digest = hashlib.sha256()
    with path.open("rb") as data:
        for chunk in iter(lambda: data.read(1024 * 1024), b""):
            digest.update(chunk)
    if digest.hexdigest() != pin["sha256"]:
        raise ValueError("Browser artifact digest mismatch")
    seen = set()
    with zipfile.ZipFile(path) as archive:
        for entry in archive.infolist():
            name = PurePosixPath(entry.filename)
            kind = stat.S_IFMT(entry.external_attr >> 16)
            canonical = str(name) + ("/" if entry.is_dir() else "")
            if (name.is_absolute() or ".." in name.parts or "\\" in entry.filename
                    or not name.parts or name.parts[0] != "chrome-linux64"
                    or entry.filename != canonical
                    or entry.filename in seen or kind not in {0, stat.S_IFREG, stat.S_IFDIR}):
                raise ValueError("Unsafe browser archive entry")
            seen.add(entry.filename)
    if "chrome-linux64/chrome" not in seen:
        raise ValueError("Browser executable is missing from the artifact")


def namespace_policy(binary):
    # This path is root-owned, non-writable by the runner and has no globs.
    if not re.fullmatch(r"/opt/zoen-cft/\d+\.\d+\.\d+\.\d+-[0-9a-f]{12}/chrome-linux64/chrome", str(binary)):
        raise ValueError("Invalid pinned browser installation path")
    return f'''abi <abi/4.0>,
include <tunables/global>
profile zoen-test-chrome "{binary}" flags=(unconfined) {{
  userns,
}}
'''


def require_protected_directory(path):
    info = path.stat()
    if path.is_symlink() or not stat.S_ISDIR(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
        raise RuntimeError("Browser installation parent is not protected")


def main():
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise RuntimeError("Pinned browser setup requires the Linux x64 CI runner")
    pin = read_pin(Path(__file__).with_name("browser.json"))
    destination = Path("/opt/zoen-cft") / (pin["version"] + "-" + pin["sha256"][:12])
    binary = destination / "chrome-linux64/chrome"
    policy = Path("/etc/apparmor.d/zoen-test-chrome")
    if destination.exists() or destination.is_symlink() or policy.exists() or policy.is_symlink():
        raise RuntimeError("Pinned browser setup requires a fresh owned installation")
    with tempfile.TemporaryDirectory(prefix="zoen-cft-", dir=os.environ["RUNNER_TEMP"]) as temporary:
        folder = Path(temporary)
        archive = folder / "chrome.zip"
        # Acquisition is separate from the unchanged 10-second renderer startup oracle.
        subprocess.run([
            "curl", "--fail", "--silent", "--show-error", "--location", "--proto", "=https",
            "--proto-redir", "=https", "--connect-timeout", "15", "--max-time", "600",
            "--max-filesize", str(pin["bytes"]), "--output", str(archive), pin["url"],
        ], check=True, timeout=605)
        verify_archive(archive, pin)
        unpacked = folder / "unpacked"
        subprocess.run(["unzip", "-q", str(archive), "-d", str(unpacked)], check=True, timeout=30)
        require_protected_directory(Path("/opt"))
        if destination.parent.is_symlink():
            raise RuntimeError("Browser installation parent is not protected")
        if not destination.parent.exists():
            subprocess.run(["sudo", "install", "-d", "-o", "root", "-g", "root", "-m", "755", str(destination.parent)], check=True)
        require_protected_directory(destination.parent)
        subprocess.run(["sudo", "mv", str(unpacked), str(destination)], check=True)
        subprocess.run(["sudo", "chown", "-R", "root:root", str(destination)], check=True)
        subprocess.run(["sudo", "chmod", "-R", "go-w", str(destination)], check=True)
        if not binary.is_file() or binary.is_symlink() or not os.access(binary, os.X_OK):
            raise RuntimeError("Pinned browser is not executable")
        # Grant userns only to this immutable browser, never disable AppArmor globally.
        enabled = Path("/sys/module/apparmor/parameters/enabled")
        if enabled.exists() and enabled.read_text().strip() == "Y":
            source = folder / "policy"
            source.write_text(namespace_policy(binary))
            subprocess.run(["sudo", "install", "-o", "root", "-g", "root", "-m", "644", str(source), str(policy)], check=True)
            subprocess.run(["sudo", "apparmor_parser", "-r", str(policy)], check=True)
        with open(os.environ["GITHUB_ENV"], "a", encoding="utf-8") as output:
            output.write(f'CHROME_BIN={binary}\nZOEN_PINNED_CHROME_VERSION={pin["version"]}\n')
    print(json.dumps({"event": "pinnedChromeInstalled", "version": pin["version"], "archiveSha256": pin["sha256"], "platform": pin["platform"]}))


if __name__ == "__main__":
    main()
