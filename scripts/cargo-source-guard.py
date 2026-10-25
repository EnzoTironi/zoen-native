#!/usr/bin/env python3
"""Source-bound Cargo invocations in an exclusively leased, cooperating cache."""

import argparse
import fcntl
import hashlib
import json
import os
import pathlib
import runpy
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
if sys.version_info < (3, 11):
    raise SystemExit("cargo-source-guard requires Python 3.11+ (stdlib tomllib)")

# Load the bound helper source without consulting timestamp-based bytecode caches.
CONFIGURATION_HELPER = pathlib.Path(__file__).with_name("cargo_guard_configuration.py")
_configuration = runpy.run_path(str(CONFIGURATION_HELPER))
cargo_configuration = _configuration["cargo_configuration"]
compilation_scope = _configuration["compilation_scope"]


class GuardError(Exception):
    pass


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def file_hash(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def capture(command, cwd, env):
    result = subprocess.run(command, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise GuardError("Cargo/Git metadata or toolchain query failed: " + command[0])
    return result.stdout.decode()


def atomic_json(path, value):
    fd, temporary = tempfile.mkstemp(prefix="state-", dir=path.parent)
    try:
        with os.fdopen(fd, "w") as stream:
            json.dump(value, stream, sort_keys=True)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def safe_directory(path):
    try:
        path.mkdir(mode=0o700)
    except FileExistsError:
        pass
    info = path.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise GuardError("Guard sidecar must be an owned private directory, not a symlink")


def inside(path, directory):
    return path == directory or directory in path.parents



def snapshot(cargo, manifest, target, sidecar, receipt, env, extra_env, extra_files):
    workspace = pathlib.Path(manifest).parent
    metadata = json.loads(capture([cargo, "metadata", "--offline", "--locked", "--all-features", "--format-version", "1", "--manifest-path", str(manifest)], workspace, env))
    workspace = pathlib.Path(metadata["workspace_root"]).resolve()
    if pathlib.Path(metadata["target_directory"]).resolve() != target:
        raise GuardError("Cargo metadata selected a different target directory")
    local = [p for p in metadata["packages"] if p["source"] is None]
    if not local:
        raise GuardError("No local Cargo packages to bind")
    collisions = {p["name"] for p in local} & {p["name"] for p in metadata["packages"] if p["source"] is not None}
    if collisions:
        raise GuardError("Local/third-party package name collision; Cargo clean cannot isolate these sources")
    packages = {}
    for package in local:
        package_manifest = pathlib.Path(package["manifest_path"]).resolve(strict=True)
        directory = package_manifest.parent
        git_root = pathlib.Path(capture(["git", "rev-parse", "--show-toplevel"], directory, env).strip()).resolve()
        relative = directory.relative_to(git_root)
        names = capture(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", str(relative)], git_root, env).split("\0")
        inputs = {}
        for name in sorted(set(filter(None, names))):
            path = git_root / name
            resolved = path.resolve()
            if any(inside(resolved, excluded) for excluded in (target, sidecar)) or resolved == receipt:
                continue
            if not path.is_file():
                raise GuardError("Missing/non-file Git source input: " + str(path))
            inputs[str(path)] = {"sha256": file_hash(path), "symlink": os.readlink(path) if path.is_symlink() else None}
        if str(package_manifest) not in inputs:
            raise GuardError("Local manifest is not a Git-visible source input")
        explicit = {str(pathlib.Path(path).resolve()) for path in extra_files}
        if any(str(pathlib.Path(item["src_path"]).resolve()) not in inputs and str(pathlib.Path(item["src_path"]).resolve()) not in explicit for item in package["targets"]):
            raise GuardError("A Cargo target source is not Git-visible; declare it with --input-file")
        packages[package["id"]] = {"name": package["name"], "manifest": str(package_manifest), "inputs": inputs, "digest": digest(inputs)}
    git_root = pathlib.Path(capture(["git", "rev-parse", "--show-toplevel"], workspace, env).strip()).resolve()
    config, configured_targets = cargo_configuration(manifest.parent, env, file_hash)
    directory = manifest.parent
    while True:
        for name in (".cargo/config", ".cargo/config.toml", "rust-toolchain", "rust-toolchain.toml"):
            path = directory / name
            if path.is_file():
                config[str(path)] = file_hash(path)
        if directory == git_root:
            break
        if not inside(directory, git_root):
            raise GuardError("Workspace is outside its Git root")
        directory = directory.parent
    for name in ("Cargo.toml", "Cargo.lock"):
        path = workspace / name
        if not path.is_file():
            raise GuardError("Qualification requires an existing Cargo manifest and lockfile")
        config[str(path)] = file_hash(path)
    for filename in extra_files:
        path = pathlib.Path(filename).resolve(strict=True)
        config[str(path)] = file_hash(path)
    names = set(extra_env) | {"RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "RUSTUP_TOOLCHAIN", "CARGO_HOME", "CARGO_BUILD_TARGET", "CARGO_INCREMENTAL", "CC", "CXX", "CFLAGS", "CXXFLAGS", "MACOSX_DEPLOYMENT_TARGET", "IPHONEOS_DEPLOYMENT_TARGET"}
    names.update(name for name in env if name.startswith(("CARGO_PROFILE_", "CARGO_TARGET_", "CARGO_BUILD_")))
    programs = {}
    for name in ("RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER"):
        if env.get(name):
            path = pathlib.Path(shutil.which(env[name]) or env[name]).resolve(strict=True)
            programs[name] = {"path": str(path), "sha256": file_hash(path)}
    context = {"workspace": str(workspace), "manifest": str(manifest), "target": str(target), "config": config, "configuredTargets": configured_targets, "guard": file_hash(pathlib.Path(__file__)), "configurationHelper": file_hash(CONFIGURATION_HELPER), "cargo": capture([cargo, "-Vv"], workspace, env), "rustc": capture([env.get("RUSTC") or "rustc", "-Vv"], workspace, env), "compilerPrograms": programs, "environment": {name: digest(env[name]) if name in env else None for name in sorted(names)}}
    return {"context": context, "contextDigest": digest(context), "packages": packages, "thirdPartyNames": sorted({p["name"] for p in metadata["packages"] if p["source"] is not None}), "thirdPartyIds": sorted(digest(p["id"]) for p in metadata["packages"] if p["source"] is not None)}


def stop_owned(child):
    if child is None:
        return
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        child.poll()
        try:
            os.killpg(child.pid, 0)
        except ProcessLookupError:
            break
        time.sleep(0.02)
    else:
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    child.wait()


def cargo_command(command, cwd, env, lock_fd, launched):
    child = subprocess.Popen(command, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True, pass_fds=(lock_fd,))
    launched(child.pid)
    records = []
    try:
        for line in iter(lambda: child.stdout.readline(16 * 1024 * 1024 + 1), b""):
            if len(line) > 16 * 1024 * 1024:
                raise GuardError("Cargo output record exceeds the capture bound")
            try:
                sys.stdout.buffer.write(line)
                sys.stdout.buffer.flush()
            except (BrokenPipeError, OSError):
                raise GuardError("Cargo output forwarding failed")
            try:
                record = json.loads(line)
            except (ValueError, UnicodeDecodeError):
                continue
            if isinstance(record, dict) and record.get("reason") == "compiler-artifact":
                records.append(record)
        code = child.wait()
        if code:
            raise GuardError("Cargo command exited " + str(code))
        try:
            os.killpg(child.pid, 0)
        except ProcessLookupError:
            pass
        else:
            stop_owned(child)
            raise GuardError("Cargo left descendants; no output qualification")
        return records
    finally:
        if child.poll() is None:
            stop_owned(child)
        child.stdout.close()



def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest-path", required=True)
    parser.add_argument("--target-dir", required=True)
    parser.add_argument("--receipt", required=True)
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--input-env", action="append", default=[])
    parser.add_argument("--input-file", action="append", default=[])
    parser.add_argument("--adopt-idle-target", action="store_true")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command or command[0] not in ("build", "test", "check", "clippy"):
        parser.error("expected -- build|test|check|clippy [Cargo arguments]")
    cargo_args = command[1:command.index("--") if "--" in command else len(command)]
    forbidden = ("--manifest-path", "--target-dir", "--config", "--message-format", "-C", "-Z")
    if any(any(arg == flag or arg.startswith(flag + "=") for flag in forbidden) or arg.startswith(("-Z", "-C", "-m")) for arg in cargo_args):
        parser.error("manifest/target/config/message-format overrides are owned by the guard")
    manifest = pathlib.Path(args.manifest_path).resolve(strict=True)
    target = pathlib.Path(args.target_dir).resolve()
    if target == target.parent or inside(manifest, target):
        parser.error("target cannot contain the source manifest")
    receipt = pathlib.Path(args.receipt).absolute()
    if receipt.exists() or receipt.is_symlink():
        parser.error("receipt must be a new file")
    target.parent.mkdir(parents=True, exist_ok=True)
    sidecar = target.parent / (".cargo-source-guard-" + digest(str(target))[:24])
    result = {"schema": 1, "status": "FAIL", "target": str(target), "command": command}
    receipt_fd = os.open(str(receipt), os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    lock_fd = None
    state_path = sidecar / "state.json"
    try:
        safe_directory(sidecar)
        lock_fd = os.open(str(sidecar / "lease"), os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
        info = os.fstat(lock_fd)
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_nlink != 1 or info.st_mode & 0o077:
            raise GuardError("Unsafe target lease file")
        try:
            fcntl.flock(lock_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise GuardError("Target is busy in another guarded invocation")
        previous = None
        if state_path.exists() or state_path.is_symlink():
            info = state_path.lstat()
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
                raise GuardError("Unsafe source stamp")
            try:
                previous = json.loads(state_path.read_text())
            except ValueError:
                previous = None
        valid = (isinstance(previous, dict) and previous.get("schema") == 1
                 and previous.get("target") == str(target) and isinstance(previous.get("scopes"), dict)
                 and isinstance(previous.get("thirdPartyNames"), list)
                 and all(isinstance(name, str) and name for name in previous["thirdPartyNames"]))
        if not valid and target.exists() and any(target.iterdir()) and not args.adopt_idle_target:
            raise GuardError("Unmanaged nonempty target; source stamp invalid or absent; confirm it is idle with --adopt-idle-target")
        env = os.environ.copy()
        env["CARGO_TARGET_DIR"] = str(target)
        env["CARGO_BUILD_BUILD_DIR"] = str(target)
        env["CARGO_TERM_COLOR"] = "never"
        before = snapshot(args.cargo, manifest, target, sidecar, receipt, env, args.input_env, args.input_file)
        result["before"] = before
        scope = compilation_scope(cargo_args, before["context"]["configuredTargets"])
        result["scope"] = scope
        scope_key = digest(scope)
        # SDK/toolchain/environment belong to each target/profile scope. A
        # host -> simulator -> host build must not erase the other bindings.
        # Real source/config/tooling changes still retire every prior scope.
        context = before["context"]
        owner_source = digest({
            "workspace": context["workspace"], "manifest": context["manifest"],
            "config": context["config"], "guard": context["guard"],
            "configurationHelper": context["configurationHelper"],
            "packages": {key: value["digest"] for key, value in before["packages"].items()},
        })
        third_party_names = set(before["thirdPartyNames"]) | set(previous.get("thirdPartyNames", []) if valid else [])
        if third_party_names & {value["name"] for value in before["packages"].values()}:
            raise GuardError("Local package name collides with recorded third-party cache history")
        prior_scope = previous["scopes"].get(scope_key, {}) if valid else {}
        if not isinstance(prior_scope, dict):
            prior_scope = {}
        trusted = prior_scope.get("status") == "bound" and prior_scope.get("contextDigest") == before["contextDigest"] and set(prior_scope.get("packages", {})) == set(before["packages"])
        clean = [key for key, value in before["packages"].items() if not trusted or prior_scope["packages"][key]["digest"] != value["digest"]]
        result["cleanedPackages"] = clean
        scopes = previous["scopes"].copy() if valid and previous.get("ownerSourceDigest") == owner_source else {}
        scopes[scope_key] = {"status": "in_progress"}
        working = {"schema": 1, "status": "in_progress", "target": str(target), "pid": os.getpid(), "ownerSourceDigest": owner_source, "thirdPartyNames": sorted(third_party_names), "scopes": scopes}
        atomic_json(state_path, working)
        for name in sorted({before["packages"][key]["name"] for key in clean}):
            for triple in scope["targets"] or [None]:
                target_args = ["--target", triple] if triple else []
                cleaned = subprocess.run([args.cargo, "clean", "--offline", "--locked", "--manifest-path", str(manifest), "--profile", scope["profile"], "--package", name, *target_args], cwd=manifest.parent, env=env, pass_fds=(lock_fd,))
                if cleaned.returncode:
                    raise GuardError("Local package output invalidation failed")
        def launched(pid):
            working["cargoPid"] = pid
            atomic_json(state_path, working)
        invocation = [args.cargo, command[0], "--offline", "--locked", "--manifest-path", str(manifest), "--target-dir", str(target), "--message-format=json-render-diagnostics", *command[1:]]
        records = cargo_command(invocation, manifest.parent, env, lock_fd, launched)
        after = snapshot(args.cargo, manifest, target, sidecar, receipt, env, args.input_env, args.input_file)
        result["after"] = after
        if before != after:
            raise GuardError("Source/configuration changed during Cargo; outputs remain unqualified")
        artifacts = []
        for record in records:
            package = record.get("package_id")
            if package not in before["packages"]:
                if isinstance(package, str) and digest(package) in before["thirdPartyIds"]:
                    continue
                raise GuardError("Artifact package was absent from the qualified metadata graph")
            if not isinstance(record.get("fresh"), bool) or not isinstance(record.get("filenames"), list) or not all(isinstance(value, str) for value in record["filenames"]):
                raise GuardError("Malformed compiler-artifact record")
            if pathlib.Path(record["manifest_path"]).resolve() != pathlib.Path(before["packages"][package]["manifest"]):
                raise GuardError("Artifact manifest does not match the qualified package")
            if package in clean and record.get("fresh") is not False:
                raise GuardError("Invalidated local package unexpectedly reported Fresh")
            for filename in record["filenames"]:
                path = pathlib.Path(filename).resolve(strict=True)
                if not inside(path, target) or not path.is_file():
                    raise GuardError("Local artifact is outside the owned target or not a file")
                artifacts.append({"package": package, "manifest": record["manifest_path"], "path": str(path), "sha256": file_hash(path), "fresh": record["fresh"]})
        if not artifacts:
            raise GuardError("Cargo emitted no local artifacts to qualify")
        result["artifacts"] = artifacts
        result["status"] = "PASS"
        scopes[scope_key] = {"status": "bound", "contextDigest": before["contextDigest"], "packages": {key: {"digest": value["digest"]} for key, value in before["packages"].items()}}
        atomic_json(state_path, {"schema": 1, "status": "bound", "target": str(target), "ownerSourceDigest": owner_source, "thirdPartyNames": sorted(third_party_names), "scopes": scopes})
    except (GuardError, OSError, ValueError, KeyError, TypeError, KeyboardInterrupt) as error:
        result["error"] = str(error) or "Interrupted"
    finally:
        try:
            with os.fdopen(receipt_fd, "w") as stream:
                json.dump(result, stream, indent=2, sort_keys=True)
                stream.write("\n")
        finally:
            if lock_fd is not None:
                os.close(lock_fd)
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    def interrupted(signum, _frame):
        raise GuardError("Interrupted by signal " + str(signum))
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    raise SystemExit(main())
