"""Read local package edges from Cargo metadata; never run a compilation."""
import json
from pathlib import Path
import re
import subprocess


ROOTS = {
    "android": ("client-core", "uniffi-bindgen"),
    "client": ("client-core",),
    # The acceptance job builds and executes its canonical peers, not only WASM.
    "release_web": ("zoen-web-core", "zoen-relay", "zoen-cli"),
}


def inside(root, path):
    value = Path(path).resolve()
    value.relative_to(root)
    return value


def graph(root, cargo):
    result = subprocess.run(
        [cargo, "metadata", "--no-deps", "--offline", "--locked", "--format-version", "1"],
        cwd=root, check=True, capture_output=True, timeout=60,
    )
    data = json.loads(result.stdout)
    if (not isinstance(data, dict) or type(data.get("version")) is not int
            or data["version"] != 1 or Path(data["workspace_root"]).resolve() != root):
        raise ValueError("Unsupported Cargo metadata or workspace root")
    packages, names = {}, {}
    for package in data["packages"]:
        directory = inside(root, package["manifest_path"]).parent
        if (Path(package["manifest_path"]).name != "Cargo.toml"
                or directory in packages or package["name"] in names):
            raise ValueError("Ambiguous package identity")
        if package["id"] not in data["workspace_members"]:
            raise ValueError("Unmodelled non-workspace package")
        if not isinstance(package["dependencies"], list) or not package["targets"]:
            raise ValueError("Missing package dependency/target metadata")
        for target in package["targets"]:
            inside(directory, target["src_path"])
            if (not isinstance(target["kind"], list) or not target["kind"]
                    or set(target["kind"]) - {"lib", "rlib", "dylib", "cdylib", "staticlib",
                                               "bin", "test", "bench", "example", "proc-macro", "custom-build"}):
                raise ValueError("Missing target kind")
        packages[directory] = package
        names[package["name"]] = directory
    if not packages or {p["id"] for p in packages.values()} != set(data["workspace_members"]):
        raise ValueError("Incomplete workspace metadata")
    for directory in packages:
        if any(other != directory and other in directory.parents for other in packages):
            raise ValueError("Nested package ownership is not supported")
    closures = {}
    for job, roots in ROOTS.items():
        pending = [names[name] for name in roots]
        selected = set()
        while pending:
            directory = pending.pop()
            if directory in selected:
                continue
            package = packages[directory]
            selected.add(directory)
            if any(set(target["kind"]) & {"custom-build", "proc-macro"}
                   for target in package["targets"]):
                raise ValueError("Unmodelled build-script/proc-macro inputs")
            for dependency in package["dependencies"]:
                # Optional and target-specific edges remain included. Native
                # builds exclude dev edges, exactly as build-expo-core does;
                # the types exporter includes them. Web is conservative too.
                if dependency["kind"] not in (None, "dev", "build"):
                    raise ValueError("Unknown Cargo dependency kind")
                if job == "android" and dependency["kind"] == "dev":
                    continue
                path = dependency.get("path")
                if path is not None:
                    path = inside(root, path)
                    if path not in packages:
                        raise ValueError("Missing local dependency metadata")
                    pending.append(path)
                elif dependency.get("source") is None:
                    raise ValueError("Unmodelled local dependency")
        closures[job] = selected
    return packages, closures


def compiler_inputs(root, directories):
    """Model literal compiler includes; dynamic inputs select the full lane."""
    inputs = set()
    pending = [path for directory in directories for path in directory.rglob("*.rs")]
    seen = set()
    while pending:
        path = pending.pop()
        if path in seen:
            continue
        seen.add(path)
        text = path.read_text(encoding="utf-8")
        for match in re.finditer(r"\binclude(?:_str|_bytes)?\b(?=\s*(?:!|/\*|//))", text):
            # Whitespace before ! is valid Rust. Unrecognised delimiters or
            # comments are conservative fallback, never an invisible input.
            argument = re.match(r'\s*!\s*\(\s*("(?:\\.|[^"\\])*")\s*\)', text[match.end():])
            if not argument:
                raise ValueError("Dynamic or unknown compiler include")
            target = inside(root, path.parent / json.loads(argument[1]))
            if not target.is_file():
                raise ValueError("Missing compiler input")
            inputs.add(target)
            if target.suffix == ".rs":
                pending.append(target)
        # A relative sibling module remains within its package's source tree.
        # Foreign/parent module indirection needs a dedicated input contract.
        for match in re.finditer(r'#\[\s*path\s*=\s*"([^"]+)"', text):
            if ".." in Path(match[1]).parts or Path(match[1]).is_absolute():
                raise ValueError("Unmodelled external module input")
    return inputs
