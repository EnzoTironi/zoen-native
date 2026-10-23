"""Linux exec boundary control; keep writable executable FDs out of Rust's test pool."""

import errno
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


def expect_busy(executable):
    try:
        subprocess.run(
            [str(executable), "--version"],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=2,
            check=True,
        )
    except OSError as error:
        assert error.errno == errno.ETXTBSY, f"unexpected errno: {error.errno}"
        return error.errno
    raise AssertionError("Linux accepted an executable with an open writer")


def control(source, directory):
    executable = directory / "write-open-control"
    shutil.copyfile(source, executable)
    executable.chmod(0o700)
    executable.with_suffix(".json").write_text(
        json.dumps({"mode": "version", "marker": str(directory / "turns"), "expected": None})
    )
    writer = executable.open("r+b")
    release_reader = release_writer = child = None
    try:
        direct_errno = expect_busy(executable)
        release_reader, release_writer = os.pipe()
        child = os.fork()
        if child == 0:
            # EOF also releases this child if the bounded parent helper is killed.
            os.close(release_writer)
            try:
                os.read(release_reader, 1)
            finally:
                writer.close()
                os.close(release_reader)
            os._exit(0)
        os.close(release_reader)
        release_reader = None
        writer.close()
        # A controlled fork inherited the writer. Closing only our copy is insufficient.
        inherited_errno = expect_busy(executable)
        os.close(release_writer)
        release_writer = None
        _, status = os.waitpid(child, 0)
        child = None
        assert os.waitstatus_to_exitcode(status) == 0, "writer child failed"
        version = subprocess.run(
            [str(executable), "--version"],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=2,
            check=True,
            text=True,
        ).stdout
        assert version == "codex-cli 0.162.0\n", "unexpected control version"
        return {"busy_errno": direct_errno, "inherited_errno": inherited_errno, "version": version}
    finally:
        writer.close()
        for descriptor in (release_reader, release_writer):
            if descriptor is not None:
                os.close(descriptor)
        if child is not None:
            os.waitpid(child, 0)


if __name__ == "__main__":
    try:
        print(json.dumps(control(Path(sys.argv[1]), Path(sys.argv[2]))), flush=True)
    except Exception as error:
        # Report only bounded diagnostic classes, never source paths or environment.
        print(json.dumps({"error": type(error).__name__, "errno": getattr(error, "errno", None)}))
        sys.exit(1)
