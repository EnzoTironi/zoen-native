"""Exercise the real Git index, commits and installer in disposable repositories."""

import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[2]
HOOK = ROOT / ".githooks" / "pre-commit"
INSTALLER = ROOT / "scripts" / "install-git-hooks.py"
AUDIT = ROOT / "scripts" / "check-file-sizes.py"


class GitFixture:
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.repo = Path(self.temporary.name) / "repo"
        self.repo.mkdir()
        self.env = {key: value for key, value in os.environ.items()
                    if not key.startswith("GIT_")}
        self.env.update({
            "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_AUTHOR_NAME": "Hook Test", "GIT_AUTHOR_EMAIL": "hook@example.invalid",
            "GIT_COMMITTER_NAME": "Hook Test", "GIT_COMMITTER_EMAIL": "hook@example.invalid",
        })
        self.git("init", "--quiet")
        self.git("config", "commit.gpgsign", "false")
        (self.repo / ".githooks").mkdir()
        (self.repo / "scripts").mkdir()
        shutil.copyfile(HOOK, self.repo / ".githooks" / "pre-commit")
        shutil.copyfile(INSTALLER, self.repo / "scripts" / "install-git-hooks.py")
        if AUDIT.exists():
            shutil.copyfile(AUDIT, self.repo / "scripts" / "check-file-sizes.py")

    def run_command(self, args, check=True, cwd=None):
        result = subprocess.run(args, cwd=cwd or self.repo, env=self.env,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if check and result.returncode:
            self.fail("Command failed: {!r}\n{}".format(args, result.stderr.decode()))
        return result

    def git(self, *args, check=True):
        return self.run_command(["git", *args], check=check)

    def install(self, check=True):
        return self.run_command([sys.executable, "scripts/install-git-hooks.py"], check=check)

    def write(self, name, data, stage=True):
        path = self.repo / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        if stage:
            self.git("add", "--", name)
        return path

    def hook(self):
        # Use the interpreter explicitly so tests do not depend on checkout mode bits.
        return self.run_command([sys.executable, ".githooks/pre-commit"], check=False)

    def audit(self):
        return self.run_command([sys.executable, "scripts/check-file-sizes.py", "--json"],
                                check=False)

