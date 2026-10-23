# Expo Core with an explicit shared Cargo target

`scripts/build-expo-core.mjs types|ios|android` routes builds through the existing
`cargo-source-guard.py` when `CARGO_TARGET_DIR` is explicitly set. Python 3.11+
is required; select it with `ZOEN_CARGO_GUARD_PYTHON` if `python3` is older.
The default, unset-target path retains its existing isolated Cargo behavior.

The guard owns the lease, source/configuration stamp, local-workspace invalidation
and third-party cache preservation. These are cooperative cache guarantees,
not a security sandbox against an unrelated process writing into the target.
The builder does not automatically adopt a cache or wait for another owner's lease.
An unknown nonempty cache fails with the guard's diagnostic and a receipt path.

After confirming that such a target is idle, an operator can explicitly adopt it
with the public guard. For example, from the workspace root:

```sh
python3 scripts/cargo-source-guard.py \
  --manifest-path "$PWD/Cargo.toml" --target-dir "$CARGO_TARGET_DIR" \
  --receipt /absolute/new/adoption-receipt.json --adopt-idle-target \
  -- build --locked -p client-core
```

The receipt's parent must already exist and its filename must be new. This is an
explicit operator decision, not an instruction to adopt a target that is busy.
The actual builder will then qualify its own commands and configurations.
Guarded builds are offline/locked; acquire missing dependencies separately.

Each invocation creates a private directory beneath
`apps/zoen/modules/zoen-core/generated/cargo-source-guard/`. Its individual guard
receipts bind the actual Cargo manifest, source inputs, toolchain, configuration
and output hashes. The builder declares its tooling files and the SDK/deployment,
library search and DeckCraft resource-manifest environment inputs. Guarded builds
consolidate `CARGO_BUILD_BUILD_DIR` into the selected target, as the guard requires.
No environment values are added to builder receipts; the guard records hashes.
Artifact lookup uses the package manifest paths returned by locked/offline Cargo
metadata, including tools outside `crates/`; it does not infer package directories.

The builder verifies and copies each required executable/library into that private
directory before consuming it. Hash mismatch, missing/ambiguous output or changed
workspace inputs between commands refuses qualification. This protects later
generator/framework steps from a cooperating build replacing shared target files
after a command releases its lease. Executable copies are owner-executable and
read-only; library copies are owner-readable and read-only. There is no automatic
retry or cleanup of another invocation's receipt/artifact directory.
Use one builder per checkout's generated-output tree; the Cargo lease does not
serialize two generators writing into the same checkout. Private copies and
receipts can be reclaimed explicitly after their outputs are no longer needed.

The types exporter is still a dev-profile example with `client-types`. The iOS
host/device/simulator builds retain their selected profiles, SDKs and deployment
target, while the binding generator retains its original dev profile. Generated
names, exact framework inventories and the resource-manifest input remain the
same. A guarded invocation always requalifies its Cargo commands rather than
accepting an older generated-output stamp. On success the existing platform
stamp references `qualification.json`; on failure it is not replaced. Initial
guard/build failure leaves the previous generated iOS framework intact.

Android's explicit shared-target path also guards the host library and binding
generator, using their actual metadata-derived manifests and private copies.
Its `cargo-ndk 4.1.2` adapter admits only the pinned tool's exact metadata/version
queries followed by one `build --lib` with the requested package, profile and ABI.
Read-only queries accept the closed stdin supplied by cargo_metadata's
`Command.output()`; only compilation treats its inherited control-pipe EOF as
cancellation. Signal cancellation and refusal of post-build queries remain.
The Core manifest already declares `cdylib`; this emits the required `.so` through
ordinary Cargo's artifact stream without changing Core's crate types. The
unchanged guard itself does not execute `cargo ndk` or `cargo rustc`.

The proxy is installed as private `CARGO` **inside** cargo-ndk's completed build
environment. It delegates to the real Cargo executable, never recursively through
itself. Only the exact NDK-added manifest/target/message-format suffix and the
fixed `--locked` duplicate already supplied by the guard are normalized. Unknown flags, argument overrides, duplicate builds, post-build
queries, unsupported cargo-ndk versions and mismatched API/ABI fail closed. The
current API remains 28 and supported ABIs remain arm64-v8a and x86_64. The unset
`CARGO_TARGET_DIR` shell pipeline keeps its previous commands and behavior.

Effective NDK compiler/linker/runner selections, target flags, API, sysroot paths
and related environment are declared to the canonical guard, including the
`TARGET_CC/CXX/AR/RANLIB/CFLAGS/CXXFLAGS` selections used by the pinned
cc_env precedence. Receipts contain
environment hashes rather than an environment dump. Tool input hashes include
real Cargo, cargo-ndk, the private proxy, Node, selected compiler/binutils and
`source.properties`/CMake toolchain selection. The **pinned SDK/NDK installation
must remain immutable during qualification**. This does not hash the whole SDK or
attest manual, unversioned sysroot edits; the analogous Rust standard-library and
Apple SDK installation assumptions also remain. Configuring tool wrappers which
cannot be resolved to existing absolute executables is refused in this scoped
path rather than silently leaving an effective tool unbound.

There is no NDK `-o` output, and `CARGO_NDK_OUTPUT_PATH` is refused even if empty:
that environment variable would otherwise enable shared-artifact copying. Only
receipt-validated private `.so` copies enter a staged Android tree; copied bytes
are checked again before promotion. Existing Kotlin/JNI metadata and checksum
validation still runs. On success `core-contract.json` references the private
Android qualification and its hash; verification refuses a missing/changed
referenced qualification. Failure does not replace the previous generated tree.
These receipts are evidence, not another target ownership stamp or durable cache
authority. Do not delete referenced proof directories while using their outputs.

SIGINT/SIGTERM or the owned outer control pipe's EOF retires the proxy, which
signals and awaits the existing guard. If NDK exits early, the outer builder
checks the proxy's private cleanup acknowledgement and disappearance before
returning its failure. Its 10-second observation bound is not a Cargo build or
product timeout; expiry fails without promotion or a claim of successful reap.
A direct **SIGKILL of the proxy cannot be propagated** by this unchanged guard:
the inherited canonical lease continues to exclude another cooperating build
until the owned Cargo process exits, but immediate descendant cleanup is not
claimed. No private result from a failed outer invocation is promoted. The
cooperative guard is not a sandbox against uncooperative target writers.

This integration is not an Android app, signing, device, runtime or parity claim.

Entrypoint checks (tiny Rust fixture only, no Core/native build):

```sh
ZOEN_CARGO_GUARD_PYTHON=/path/to/python3.12 \
  node --test --test-concurrency=1 scripts/tests/build-expo-core.test.mjs \
    scripts/tests/expo-ios-orchestration.test.mjs
```

The Rust CI job runs both files with Node 24 after the guard's Python tests. Its
locked metadata preparation handles a cold registry cache without compiling Core.

The adapter and orchestration port checks need no NDK (the latter queries real
repository Cargo metadata, but fakes compiler/generator ports):

```sh
node --test scripts/tests/expo-android-proxy.test.mjs \
  scripts/tests/expo-android-orchestration.test.mjs
```

The dependency-free real Android fixture requires explicit admission, pinned
cargo-ndk 4.1.2, an installed NDK and matching Rust targets. It compiles synthetic
ELF libraries, not Core, and never executes an Android binary:

```sh
ZOEN_TEST_ANDROID_SHARED_CARGO=1 ZOEN_TEST_ANDROID_ABIS=both \
  node --test --test-concurrency=1 scripts/tests/expo-android-real.test.mjs
```

The Expo Android job supplies those prerequisites at its existing pinned versions.
It exercises A→B worktree switching with old mtimes, restored-mtime source edits,
environment drift, API refusal, both ELF machine identities, unknown nonempty
cache/held-lease refusal, private-copy survival and owned cancellation. Without
explicit admission the real fixture is visibly skipped, which is not Android
qualification. Source-only preparation and tests actually executed must be
reported separately.
