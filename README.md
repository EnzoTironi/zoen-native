# Zoen

Talk to people and agents in the same place. Keep the plans, pages, files and mini-apps produced by the conversation under your control.

© 2026 Enzo Tironi. All rights reserved. Public for viewing only. See [LICENSE](LICENSE).

Zoen is being built as a complete product for one billion monthly users. The shared Rust core powers native iPhone and Mac clients. Native Android is under review in [PR 41](https://github.com/EnzoTironi/zoen-native/pull/41). The web shell is under review in [PR 44](https://github.com/EnzoTironi/zoen-native/pull/44); authenticated browser messaging remains unfinished.

## Current progress

This snapshot uses `main` at `f277c804`, including encrypted recovery [PR 45](https://github.com/EnzoTironi/zoen-native/pull/45) and the reviewed model gateway [PR 49](https://github.com/EnzoTironi/zoen-native/pull/49), on 10 October 2026. Work published in parallel remains identified by its own commit and PR.

| Area | Current implementation and evidence | Still required |
| --- | --- | --- |
| Messaging | Real accounts, signed events, durable outbox, relay sync and OpenMLS encryption for direct and group conversations. A local durable key-package claim candidate passed its causal reply-loss journey. | The unchanged 50-group refill journey, exact-main CLI baseline, repeatable release journeys and supported-client upgrades. |
| Devices and recovery | Explicit enrollment, revocation, encrypted history transfer, encrypted backups and recovery after loss of every original MLS device. Backend changes are integrated. | Native recovery and device-management UI, fresh Android runtime proof, production backup authority. |
| Relay | Persisted renewable ownership leases, transaction fencing, bounded forwarding and retry deduplication from [PR 43](https://github.com/EnzoTironi/zoen-native/pull/43). | Production TLS workloads, cell placement, regional recovery, sustained capacity and full cost evidence. |
| iOS and Mac | Native clients and shared themes. [PRs 38](https://github.com/EnzoTironi/zoen-native/pull/38) and [44](https://github.com/EnzoTironi/zoen-native/pull/44) add one Chats inbox, reading-position preservation, pinned widgets and Cards/List notifications. Ten focused iOS journeys passed on the combined UI source. | Fresh checks on the integration heads, release/device coverage and the remaining product flows. |
| Android | Kotlin/Compose client with shared Rust bindings, native Keystore, encrypted relay/media journeys and adaptive layouts in PR 41, at `1901c1e`. | Exact-head API 28 failed the mascot/mini-app UI checks; API 35 was cancelled. Rust CI failed M2. Fresh successful device and service journeys remain required. |
| Pages and mini-apps | Loro block documents, native rich-text editing, versions, file viewers and sandboxed mini-app hosts. The local candidate has encrypted draft receipts, retry/restore fencing and manual Mac save/history/relaunch proof. Plans now share the pinned mini-app card pattern in PR 44. | Canonical plan Live Page integration, complete Notion-style workflows, native cross-platform/two-device proof, live bindings, collaborative/agent updates and authenticated web parity. |
| Agents | Grants, budgets, approvals and signed tool/sandbox foundations. Durable proposals ([PR 46](https://github.com/EnzoTironi/zoen-native/pull/46)), owner registration ([PR 48](https://github.com/EnzoTironi/zoen-native/pull/48)) and the financial/certified-run stack ([PRs 51–54](https://github.com/EnzoTironi/zoen-native/pull/54)) are under review. | Green full integration checks, native admission/cancellation, encrypted worker execution, approval reconciliation, custody continuity and provider billing. |
| Communication and operations | Encrypted blob foundations, infrastructure, migrations and telemetry. | Apple photo/voice transport, push, community moderation, catalog/commerce and tested deployment/restore procedures. |

The [roadmap](docs/roadmap-status.md) records exact versions, failures and completion gates. [PR 50](https://github.com/EnzoTironi/zoen-native/pull/50) passed full Linux CI at the temporary diagnostic head `10e5b1d`, including all ten unchanged M2 journeys. Its current head `f6936e9` removes that instrumentation and has the exact production tree of `5a4d5f0`; fresh full CI is running. Successful libtest output hid the Linux census, so the diagnostic pass does not explain earlier intermittent failures or certify the uninstrumented head.

The local Plan/Page experiment passed all 61 document and 118 FFI tests before a test-only Clippy repair. After that repair, all 24 session fixtures and all-targets document/FFI Clippy passed. These tests exercise real Loro, encrypted journals, atomic SQLite failures, retries and an MLS peer; the new Plan session is still compiled only for tests. Production session/DTO/caller integration and native Plan editing remain unfinished. Earlier Page-editor evidence includes 95 Gradle unit tests, a Mac save/history/restore/relaunch journey preserving exact Unicode at v5, and ten Apple encrypted-checkpoint journeys. Release Keychain, real IME, iOS file protection, Android ABI/device execution and independent-device editor transport remain unverified. The [Live Page contract](docs/product/live-pages.md) tracks the remaining workflows.

[PR 49](https://github.com/EnzoTironi/zoen-native/pull/49) merged after reviewed fixes and successful exact-head CI. [PR 51](https://github.com/EnzoTironi/zoen-native/pull/51) passed its Linux financial target, including 100 concurrent reservations, after 30 local real-service journeys. At `be659d9`, [PR 54](https://github.com/EnzoTironi/zoen-native/pull/54) passed 87 FFI tests, 24 gateway tests and both combined runtime targets in Linux CI. The complete run still failed inherited growth, claim and M2 targets. Native admission/cancellation is a separate local backend candidate whose real-service validation is in progress. Production custody and external restore continuity remain open.

A merge or screenshot does not establish production readiness. The public staging relay was last observed advertising protocol 2; current clients require protocol 4.

## Product navigation

Chats is the inbox for direct conversations, groups and communities. Optional filters narrow it. Community permissions and shared resources belong inside the conversation. Notifications open as cards, with an explicit switch to a list and back. Desktop and web follow the iOS themes and chat-header behavior.

These changes are demonstrated in the [native interaction evidence](https://github.com/EnzoTironi/zoen-native/pull/44#issuecomment-6098239059). They are under review in PR 44. The browser demonstration currently uses sample data.

## Development

The repository contains the Rust workspace, Apple clients, relay and infrastructure. Android and the reusable web shell are in their linked PRs until integration.

```sh
rustup toolchain install 1.99.0 --profile minimal --component rustfmt,clippy
export RUSTUP_TOOLCHAIN=1.99.0
cargo fmt --all --check
scripts/build-core.sh
```

Run `scripts/dev-stack.sh` in a separate terminal; it remains attached to the relay process. Apple builds require Xcode, XcodeGen and the Rust Apple targets. The full Rust journey suite also requires PostgreSQL, FoundationDB, NATS and the sandbox providers configured by [CI](.github/workflows/ci.yml). Read [development setup](docs/dev/setup.md) before running account or device journeys.

## Documentation

HIG release acceptance and current verification gaps are tracked in the [HIG matrix](docs/product/hig-compliance.md). The [local source auditor](tools/hig-audit/README.md) is advisory.

Start with the [documentation index](docs/README.md), [current roadmap](docs/roadmap-status.md), [architecture](docs/architecture.md) and [system design](docs/system-design.md). ADRs explain decisions. Research and dated audits record their original evidence; they do not define current completion.

The product and built-in agent are both Zoen. Technical `roda-*`, `RodaCore` and related identifiers remain in packages, generated bindings, build tasks and compatibility formats. The [naming migration](docs/dev/naming.md) lists their replacements and the checks required to preserve stored data and signed history; that migration is unfinished.
