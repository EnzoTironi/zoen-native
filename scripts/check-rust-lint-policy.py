#!/usr/bin/env python3
"""Refuse workspace members that silently opt out of the shared Rust lint policy."""
from pathlib import Path
import sys
import tomllib


def violations(root: Path) -> list[str]:
    workspace = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]
    errors = []
    members = set()
    for pattern in workspace["members"]:
        paths = sorted(root.glob(pattern))
        if not paths:
            errors.append(f"Workspace member pattern has no matches: {pattern}")
        members.update(paths)
    for member in sorted(members):
        manifest = member / "Cargo.toml"
        data = tomllib.loads(manifest.read_text())
        if data.get("lints") != {"workspace": True}:
            errors.append(
                f"{manifest.relative_to(root)} must inherit [lints] workspace = true; "
                "fix the diagnostic instead of replacing the shared policy."
            )
    policy = workspace.get("lints", {})
    rust = policy.get("rust", {})
    for name in ("unused_must_use", "unsafe_op_in_unsafe_fn"):
        if rust.get(name) != "deny":
            errors.append(f"workspace.lints.rust.{name} must remain deny")
    clippy = policy.get("clippy", {})
    for name, level in (("all", "warn"), ("pedantic", "warn"), ("perf", "warn"),
                        ("correctness", "deny")):
        if clippy.get(name) != {"level": level, "priority": -1}:
            errors.append(f"workspace.lints.clippy.{name} must remain enabled at {level}/-1")
    for name in ("unwrap_used", "expect_used", "dbg_macro", "todo", "unimplemented",
                 "await_holding_lock", "undocumented_unsafe_blocks"):
        if clippy.get(name) != "deny":
            errors.append(f"workspace.lints.clippy.{name} must remain deny")
    for name in ("redundant_clone", "significant_drop_in_scrutinee"):
        if clippy.get(name) != "warn":
            errors.append(f"workspace.lints.clippy.{name} must remain warn")
    for name in ("restriction", "nursery"):
        if name in clippy:
            errors.append(f"Select individual {name} rules; do not configure the entire group")
    return errors


if __name__ == "__main__":
    root = Path(__file__).resolve().parents[1]
    errors = violations(root)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        raise SystemExit(1)
    print("All workspace members inherit the shared Rust/Clippy lint policy.")
