# Testing & CI

Zoen is tested through **real journeys**: the `zoen` CLI drives the same core the apps use against a real relay, real FoundationDB, real Postgres and real NATS. UI journeys drive the actual apps on simulators and emulators. A claim of "done" is backed by evidence tied to an exact revision ([roadmap status](../roadmap-status.md)).

## Workflows

| Workflow | Runs on | What it does |
| --- | --- | --- |
| [`ci.yml`](../../.github/workflows/ci.yml) | Ubuntu 24.04 | `cargo fmt --all --check`; provisions Postgres, FoundationDB and NATS; installs gVisor and Firecracker for the sandbox, microVM and browser journeys; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace --no-fail-fast`. |
| [`android.yml`](../../.github/workflows/android.yml) | Ubuntu 24.04 + KVM | SDK 36 / NDK 27; builds both ABIs, unit tests and lint; verifies generated Kotlin and JNI against current source; runs native journeys on **Android 28 and 35** against an isolated relay; uploads APKs and reports. |
| [`apple-design.yml`](../../.github/workflows/apple-design.yml) | macOS 26 | Glass shader and style checks for iOS and macOS (`scripts/check-apple-glass.sh`). |
| [`web.yml`](../../.github/workflows/web.yml) | Ubuntu 24.04 | Web client typecheck and behaviour tests. |
| [`messenger-search-edit.yml`](../../.github/workflows/messenger-search-edit.yml) | Ubuntu 24.04 | Web behaviour for search, edit and delete. |

## Running tests locally

Formatting and lint (the toolchain is pinned):

```sh
rustup toolchain install 1.99.0 --profile minimal --component rustfmt,clippy
export RUSTUP_TOOLCHAIN=1.99.0
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

Service journeys need private FoundationDB and NATS (scripts provided) plus a Postgres URL:

```sh
scripts/fdb.sh up && eval "$(scripts/fdb.sh env)"
scripts/nats.sh up && eval "$(scripts/nats.sh env)"
ZOEN_TEST_PG=postgres://user@host/postgres cargo test -p zoen-cli -p zoen-relay
```

A host without Postgres, FoundationDB, NATS, gVisor and Firecracker cannot validate the complete suite with `cargo test` alone — use [`ci.yml`](../../.github/workflows/ci.yml) as the executable setup reference.

Focused suites:

```sh
cargo test --locked -j2 -p roda-ffi --lib mls::tests -- --test-threads=1
cargo test --locked -j2 -p roda-ffi --lib runtime_core::tests -- --test-threads=1
ZOEN_TEST_PG=… cargo test -p zoen-cli --test journey_agent_link
cargo test -p roda-mls --features portable
cargo test -p zoen-web-core
```

Web and UI packages:

```sh
npm ci --prefix packages/zoen-ui --ignore-scripts
npm run test:shell --prefix packages/zoen-ui
(cd packages/zoen-ui && npm run typecheck:web && npm run test:web)
```

Android (from `android/`): `./gradlew :app:testDebugUnitTest :app:lintDebug` and `./gradlew :app:connectedDebugAndroidTest`. For encrypted two-account journeys pass `-Pandroid.testInstrumentationRunnerArguments.zoenRelay=http://10.0.2.2:8787` with a local relay running.

Apple simulator journey (relay + CLI person + XCUITest, recorded):

```sh
scripts/journey-sim.sh
```

## Journey tests

End-to-end journeys live in `crates/zoen-cli/tests/journey_*.rs` (accounts and sync, encryption, device revocation, backup, files, approvals, agent citizens and links, key packages, limits, telemetry and more) and `crates/zoen-agentd/tests/journey_*.rs` (citizens, WASM, sandboxes, Fly). Each one spins up an isolated world and uses real processes.

## Markdown import performance

`journey_files` imports **every Markdown file under `docs/`** as pages through relay acceptance, exports them on another device and compares the bytes. Adding documentation therefore grows its corpus. The default check is the *shape* (no later batch may cost much more per page than the first); a wall-clock budget is opt-in with `ZOEN_IMPORT_BUDGET_MS`, and extra corpora can be added with `ZOEN_MD_CORPUS`.

```sh
cargo test --locked -j2 -p zoen-cli --test journey_files -- --test-threads=1 --nocapture
```

To compare revisions, run it five times on each side with the same toolchain, debug settings, two build jobs, disabled incremental compilation and private Postgres/FoundationDB instances; record the median and range of `IMPORT` and the byte count from `ROUND TRIP`, and profile CPU separately. The application publish window is five messages per Space, matching the MLS receiver's out-of-order tolerance.

## Other checks

- **HIG audit** — `scripts/audit-hig.sh` and [`tools/hig-audit`](../../tools/hig-audit/README.md) (advisory).
- **Load** — `crates/zoen-load` simulates people on the real wire protocol; `scripts/bench-load.sh`.
- **Gate bench** — [gate-bench](../gate-bench.md) for the safety gate.
