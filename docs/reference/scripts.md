# Scripts

Everything in `scripts/`, grouped by job. Run them from the repository root.

## Backend and local services

| Script | What it does |
| --- | --- |
| `scripts/dev-stack.sh` | The whole backend in one command: private FoundationDB + private Postgres (or `DATABASE_URL`) + `zoen-relay` on port 8787. Stays attached. Env: `ZOEN_BIND`, `ZOEN_DEV_DIR`, `ZOEN_RELAY_NAME`. |
| `scripts/fdb.sh up \| env \| down` | A private FoundationDB 7.3 for development (binaries under `.tools/fdb`, data under `.dev/fdb`, no root). `eval "$(scripts/fdb.sh env)"` exports what builds and tests need. |
| `scripts/nats.sh` | A private NATS server with JetStream (`.tools/nats`, `.dev/nats`). |
| `scripts/ci-postgres.sh` | Starts the hosted runner's PostgreSQL in CI. |
| `scripts/local-cluster.sh up \| journey \| telemetry \| down` | A local Kubernetes cell on k3d with the same manifests as staging and production. |
| `scripts/gvisor.sh` | gVisor (`runsc`) and a base root filesystem for the sandbox stand-in (Linux). |
| `scripts/firecracker.sh` | Firecracker, its jailer and a guest kernel for the microVM tier (Linux with `/dev/kvm`). |

## Building the core

| Script | What it does |
| --- | --- |
| `scripts/build-core.sh [--if-stale]` | Rust core for iPhone, simulator and Mac (arm64) + Swift bindings, packaged as the XCFramework under `apple/Packages/`. |
| `scripts/build-android-core.sh [--debug\|--release] [--abis=…]` | Rust core for Android ABIs + Kotlin bindings from the same build. |

## Running the apps

| Script | What it does |
| --- | --- |
| `scripts/run.sh [ios\|mac]` | Generates the Xcode project, builds and opens Zoen on the iPhone simulator and/or the Mac. `SIM=` picks the simulator. |
| `scripts/watch.sh [stop]` | Rebuilds only what changed and relaunches on every edit. |
| `scripts/device.sh [install]` | Zoen on your own iPhone with a free Apple ID (`ZOEN_TEAM`, `ZOEN_RELAY`). |
| `scripts/mac-iphone.sh` | The iPhone app in an iPhone-shaped window on Apple Silicon, no simulator. |
| `scripts/run-android.sh [gradle args]` | Builds the debug APK, installs it and launches it via `adb`. |

## Journeys and tests

| Script | What it does |
| --- | --- |
| `scripts/journey-sim.sh` | Relay + a CLI person + the iPhone app driven by XCUITest, recorded. |
| `scripts/journey-staging.sh` | Release app on the simulator against `relay.tryzoen.com`, with the CLI as the other person. |
| `scripts/journey-remote.sh URL` | Two CLI clients against any running relay (k3d, staging…). |
| `scripts/test-android-device.sh` | Android device test runner. |
| `scripts/test-apple-*.sh`, `scripts/test-plan-page-layout.sh` | Focused Apple journeys (message mutations, money input, page display/drafts, Plan layout). |
| `scripts/check-apple-glass.sh` | Compiles the glass shader and type-checks its SwiftUI style on iOS and macOS SDKs (CI). |
| `scripts/audit-hig.sh` | Advisory Human Interface Guidelines source audit. |
| `scripts/bench-load.sh` | Capacity runs with `zoen-load` over the real protocol. |
| `scripts/probe-connect-limit.py` | Opens N WebSocket handshakes and reports how many the relay refused. |
| `scripts/fc-measure.py` | Measures Firecracker cold boot and snapshot restore on this host. |

## Android contract and assets

`android-core-contract.py`, `check-android-upstream.py`, `test-android-core-contract.py`, `fix-android-uniffi-callbacks.py`, `test-android-uniffi-callbacks.py` bind the generated Kotlin/JNI to their exact source inputs. `export-android-*.py`, `encode-android-mascot.py` and `android-mascot-export.swift` port the original Apple art to Android. `android-codec-census.py`, `android-viewport-census.py`, `refill-census.py` and `test-android-rich-page.py` collect diagnostic evidence.
