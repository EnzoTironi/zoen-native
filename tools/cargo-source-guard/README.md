# Guarding Cargo source qualification across worktrees

`scripts/cargo-source-guard.py` prevents a cooperating shared Cargo target from
qualifying a cached executable compiled in another worktree. A before/after
source inventory alone does not establish which source produced a `Fresh` file.

Requires POSIX, Git, an already installed Cargo/Rust toolchain, cached dependency
manifests, and **Python 3.11+**. It adds no Python or Rust package dependency.

```sh
python3.12 scripts/cargo-source-guard.py \
  --manifest-path /checkout/Cargo.toml \
  --target-dir /owned-cache/target \
  --receipt /evidence/new-check.json \
  --input-env ZOEN_DECKCRAFT_RESOURCE_MANIFEST_JSON \
  -- test -p client-core --lib mls::tests:: -- --test-threads=1
```

Use `build`, `test`, `check` or `clippy`; aliases and `run` are unsupported. Pass
normal package/features/profile/target arguments after `--`. Manifest, output
directory, config and message-format overrides belong to the guard. Cargo runs
offline/locked, with intermediate and final output directories both pinned to
the selected canonical target. No whole-target cleanup or Cargo-internal layout
editing is used.

Before adopting an existing nonempty target, establish that it has no active
unguarded writers, then pass `--adopt-idle-target`. This is an operator statement
about an idle cache, not process detection. The guard refuses another guarded
invocation immediately. Keep using the guard for that target afterwards.
An unreadable JSON stamp or invalid schema/target/cache history also requires
explicit adoption before any cleanup in a nonempty cache; unknown history is
never adopted implicitly.

The owned private sidecar holds an OS lease and atomic source stamps for each
target/profile namespace. Git-visible local package inputs, Cargo configuration
hierarchy, toolchain versions, compiler wrapper files and build-environment
fingerprints bind the actual workspace. Values of environment inputs are hashed,
not recorded. `--input-env NAME` adds an application-specific compile input;
`--input-file FILE` adds ignored/external sources or build inputs. Credentials
files are never inspected.

An unknown, changed or interrupted binding invokes public `cargo clean` only for
metadata-local package names in the requested target/profile. Registry/Git
dependencies remain cached. Cargo ignores URL/version qualifiers when cleaning
packages, so local names colliding with current or previously recorded third-party
names are refused. Unrecorded history in an adopted legacy target cannot be
reconstructed; adoption must allow the stated package-name cleanup.

An unchanged qualified binding allows Cargo to reuse its outputs. A successful
receipt additionally checks compiler-artifact manifest paths, output containment
and hashes, and identical before/after inputs. Namespace stamps avoid accepting
an unchecked old release or explicit-target artifact after a host/debug build.
Other namespaces are not deleted. Cargo can still legitimately rebuild an input
such as a build script; the guard does not promise that every artifact is Fresh.

SIGTERM/interrupts stop the owned Cargo process group and leave an unqualified
stamp. Cargo inherits the lease descriptor, so killing the guard cannot permit
another guarded cleanup while that Cargo is still running. Interrupted work is
invalidated before reuse. Source/configuration changes observed during a command
also fail qualification.

This is a cooperating build guard, **not a security sandbox or hermetic build**.
Keep the worktree stable during qualification and declare ignored/external inputs.
It cannot prevent unguarded writers, a malicious same-user process, transient
change-and-restore edits, or an untrusted compiler/build script. Custom target
JSON, Windows, Cargo publish/install/package and arbitrary shell runners are not
supported. No previous qualification receipt is retroactively upgraded.

```sh
python3.12 -m unittest discover -s scripts/tests -p test_cargo_source_guard.py -v
```

Tests use tiny real Cargo projects, a synthetic checksummed vendored registry
dependency and disposable targets. They exercise the old-mtime foreign-Fresh
case, actual executable/manifest binding, dependency preservation, same-owner
edits, namespace/profile handling, configuration, leases, process death and
source changes. No Core/native build, service, provider or retained account is
needed. The raw probe records whether the installed Cargo still exhibits the
bug; an upstream fix does not make a safe guarded build fail its test.

Public Cargo interfaces used here: [package cleanup](https://doc.rust-lang.org/cargo/commands/cargo-clean.html),
[cache/output directories](https://doc.rust-lang.org/cargo/reference/build-cache.html)
and [configuration precedence](https://doc.rust-lang.org/cargo/reference/config.html).
