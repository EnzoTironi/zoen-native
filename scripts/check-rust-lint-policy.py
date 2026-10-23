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
    clippy = workspace.get("lints", {}).get("clippy", {})
    for name in ("all", "pedantic", "perf"):
        if clippy.get(name) != {"level": "warn", "priority": -1}:
            errors.append(f"workspace.lints.clippy.{name} must remain enabled at warn/-1")
    for name in ("unwrap_used", "expect_used"):
        if clippy.get(name) != "deny":
            errors.append(f"workspace.lints.clippy.{name} must remain deny")
    return errors


if __name__ == "__main__":
    root = Path(__file__).resolve().parents[1]
    errors = violations(root)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        raise SystemExit(1)
    print("All workspace members inherit the shared Rust/Clippy lint policy.")
