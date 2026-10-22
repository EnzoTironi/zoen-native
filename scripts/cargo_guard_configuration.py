"""Read Cargo invocation configuration and target/profile namespaces."""

import pathlib
import re
import tomllib


def cargo_configuration(directory, env, file_hash):
    # Cargo's documented precedence: Cargo home, then ancestors, then cwd.
    home = pathlib.Path(env.get("CARGO_HOME", str(pathlib.Path.home() / ".cargo"))).resolve()
    directories = [home] + [path / ".cargo" for path in reversed((directory, *directory.parents))]
    paths = []
    for item in directories:
        path = item / "config"
        if not path.is_file():
            path = item / "config.toml"
        if path.is_file():
            path = path.resolve()
            if path in paths:
                paths.remove(path)
            paths.append(path)
    hashes, targets = {}, None
    for path in paths:
        hashes[str(path)] = file_hash(path)
        with path.open("rb") as stream:
            value = tomllib.load(stream).get("build", {}).get("target")
        if value is not None:
            if not isinstance(value, str) and not (isinstance(value, list) and all(isinstance(item, str) for item in value)):
                raise ValueError("Unsupported Cargo build.target configuration")
            if isinstance(targets, list) and isinstance(value, list):
                targets += value
            elif targets is not None and type(targets) is not type(value):
                raise ValueError("Cargo target configuration types disagree")
            else:
                targets = value
    if env.get("CARGO_BUILD_TARGET"):
        targets = env["CARGO_BUILD_TARGET"]
    return hashes, targets


def compilation_scope(arguments, configured_targets):
    targets = []
    profile = "dev"
    index = 0
    while index < len(arguments):
        arg = arguments[index]
        if arg in ("--target", "--profile"):
            index += 1
            if index == len(arguments):
                raise ValueError("Missing target/profile value")
            value = arguments[index]
        elif arg.startswith(("--target=", "--profile=")):
            arg, value = arg.split("=", 1)
        elif arg in ("--release", "-r"):
            profile = "release"
            index += 1
            continue
        else:
            index += 1
            continue
        if not re.fullmatch(r"[A-Za-z0-9_.-]+", value):
            raise ValueError("Use explicit target triples/profile names; custom target JSON is unsupported")
        if arg == "--target":
            targets.append(value)
        else:
            profile = {"test": "dev", "bench": "release"}.get(value, value)
        index += 1
    if not targets and configured_targets is not None:
        targets = [configured_targets] if isinstance(configured_targets, str) else configured_targets
        if not targets:
            raise ValueError("Cargo has an empty target list and would build no artifacts")
        if any(not re.fullmatch(r"[A-Za-z0-9_.-]+", value) for value in targets):
            raise ValueError("Custom target JSON is unsupported; pass target triples")
    return {"targets": sorted(set(targets)), "profile": profile}
