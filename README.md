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
| Android | Kotlin/Compose client with shared Rust bindings, native Keystore, encrypted relay/media journeys and adaptive layouts in PR 41, now at `bdd8e14`. | Fresh API 28, API 35 and Rust checks are running. Earlier device/readiness failures remain recorded by their source version. |
| Pages and mini-apps | Loro block documents, native rich-text editing, versions, file viewers and sandboxed mini-app hosts. The local candidate has encrypted draft receipts, retry/restore fencing and manual Mac save/history/relaunch proof. Plans now share the pinned mini-app card pattern in PR 44. | Canonical plan Live Page integration, complete Notion-style workflows, native cross-platform/two-device proof, live bindings, collaborative/agent updates and authenticated web parity. |
| Agents | Grants, budgets, approvals and signed tool/sandbox foundations. Durable proposals ([PR 46](https://github.com/EnzoTironi/zoen-native/pull/46)) and enrolled-owner-device registration ([PR 48](https://github.com/EnzoTironi/zoen-native/pull/48)) are under review. | Green exact-head service checks, encrypted agent membership/runtime, crash-safe model/tool execution, approval reconciliation and provider billing. |
| Communication and operations | Encrypted blob foundations, infrastructure, migrations and telemetry. | Apple photo/voice transport, push, community moderation, catalog/commerce and tested deployment/restore procedures. |

The [roadmap](docs/roadmap-status.md) records exact versions, failures and completion gates. The claim candidate in [PR 50](https://github.com/EnzoTironi/zoen-native/pull/50) at `ded5650` passed growth (3/3) and durable claim (9/9) journeys in exact-head CI. The unchanged M2 suite still failed creating `Roda 30` at the eight-second synchronization deadline, before the final stock-32 gate. The older `dd03f6e` candidate passed the unchanged 50-group journey locally; that result does not certify the current Linux integration.

The local Pages candidate passed 13 document tests, 80 FFI tests and 95 Gradle unit tests across 15 suites. Its actual Mac demo saved edits, read old versions, restored as a new version and retained exact Unicode text after quitting and reopening at v5. Ten Apple encrypted-checkpoint journeys also passed, including a recreated native development vault. Automated Mac XCTest still stalls before test cases; release Keychain, real IME, iOS file protection, Android ABI/device execution and independent-device editor transport remain unverified. The [Live Page contract](docs/product/live-pages.md) includes plans and keeps comments, nested pages, sharing, live bindings and guarded agent tools unfinished.

[PR 49](https://github.com/EnzoTironi/zoen-native/pull/49) merged after reviewed fixes and successful exact-head CI. The financial runtime in [PR 51](https://github.com/EnzoTironi/zoen-native/pull/51) at `7cc433b` passed 30 local real-service journeys after a bounded reservation retry correction. Independent source review closed the cross-namespace billing defect and found no blockers in that retry delta. Earlier Linux CI at `617d5aa` failed the original 100-concurrent-reservations test with `Unavailable`; fresh CI remains required. Certified MLS/core extraction and external restore continuity still keep public production run/cancel closed.

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

Start with the [documentation index](docs/README.md), [current roadmap](docs/roadmap-status.md), [architecture](docs/architecture.md) and [system design](docs/system-design.md). ADRs explain decisions. Research and dated audits record their original evidence; they do not define current completion.

The product and built-in agent are both Zoen. Technical `roda-*`, `RodaCore` and related identifiers remain in packages, generated bindings, build tasks and compatibility formats. The [naming migration](docs/dev/naming.md) lists their replacements and the checks required to preserve stored data and signed history; that migration is unfinished.
