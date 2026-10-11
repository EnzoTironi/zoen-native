# Zoen

Talk to people and agents in the same place. Keep the plans, pages, files and mini-apps produced by the conversation under your control.

© 2026 Enzo Tironi. All rights reserved. Public for viewing only. See [LICENSE](LICENSE).

Zoen is being built as a complete product for one billion monthly users. The shared Rust core powers native iPhone and Mac clients. Native Android is under review in [PR 41](https://github.com/EnzoTironi/zoen-native/pull/41). The web shell is under review in [PR 44](https://github.com/EnzoTironi/zoen-native/pull/44); authenticated browser messaging remains unfinished.

## Current progress

This snapshot uses `main` at [`62c763b2`](https://github.com/EnzoTironi/zoen-native/commit/62c763b2d842634226d74b042e5a90c464e2ea40), including reviewed recovery, the model gateway and durable MLS claim retries from [PR 50](https://github.com/EnzoTironi/zoen-native/pull/50), on 11 October 2026. Parallel work remains identified by its own commit and checks.

| Area | Current implementation and evidence | Still required |
| --- | --- | --- |
| Messaging | Real accounts, signed events, durable outbox, relay sync and OpenMLS encryption. PR 50 is merged; fresh Linux CI passed all nine claim journeys and all ten M2 journeys, including the unchanged 50-group refill. | Production relay migration, supported-client upgrades and repeatable release journeys. |
| Devices and recovery | Explicit enrollment, revocation, encrypted history transfer, encrypted backups and recovery after loss of every original MLS device. Backend changes are integrated. | Native recovery and device-management UI, fresh Android runtime proof, production backup authority. |
| Relay | Persisted renewable ownership leases, transaction fencing, bounded forwarding and retry deduplication from [PR 43](https://github.com/EnzoTironi/zoen-native/pull/43). | Production TLS workloads, cell placement, regional recovery, sustained capacity and full cost evidence. |
| iOS and Mac | Native clients and shared themes. [PRs 38](https://github.com/EnzoTironi/zoen-native/pull/38) and [44](https://github.com/EnzoTironi/zoen-native/pull/44) add one Chats inbox, reading-position preservation, pinned widgets and Cards/List notifications. Ten focused iOS journeys passed on the combined UI source. | Fresh checks on the integration heads, release/device coverage and the remaining product flows. |
| Android | Native Kotlin/Compose client with shared Rust bindings, Keystore, encrypted media and adaptive layouts. At `e2a5406`, [PR 41](https://github.com/EnzoTironi/zoen-native/pull/41) passes Android 28, Android 35 and Rust CI. | Integration with current main, source-matched release/device evidence, public relay compatibility and remaining provider workflows. |
| Pages and mini-apps | Loro block documents, native editing, versions, file viewers and mini-app hosts. The [shared-session candidate](docs/dev/live-page-session.md) based on main `7cad5b1` passes 61 document, 7 compatibility and 120 FFI tests; public Binary Pages share editing, encrypted draft receipts, recovery and publication. Plans share the pinned mini-app card pattern in PR 44. | Regenerated client bindings, observed-context native editor adoption, canonical plan Live Pages, complete Notion-style/two-device workflows, live bindings and authenticated web parity. |
| Agents | Grants, budgets, approvals and signed tool/sandbox foundations. Durable proposals, owner registration and the financial/certified-run stack through [PR 58](https://github.com/EnzoTironi/zoen-native/pull/58) remain under review. | Full current-main checks, the C4 receipt lifetime repair, guarded publication, encrypted worker execution, custody continuity and live provider billing. |
| Communication and operations | Encrypted blob foundations, infrastructure, migrations and telemetry. | Apple photo/voice transport, push, community moderation, catalog/commerce and tested deployment/restore procedures. |

The [roadmap](docs/roadmap-status.md) records exact versions and completion gates. [PR 50's final Linux run](https://github.com/EnzoTironi/zoen-native/actions/runs/38113755244) passed formatting, workspace Clippy, 60 FFI tests, nine causal claim journeys, ten M2 journeys and the metrics target. Its tested tree matches merged main. The original eight-second CLI deadline, 50-group workload, low-water threshold of 8, stock target of 32 and five-second claim backoff are unchanged. Earlier diagnostic and failed runs remain historical evidence; they do not describe the merged candidate. This pass does not establish production deployment or billion-user capacity.

The local Plan/Page experiment subsequently passed all 61 document and 122 FFI tests, plus all-target document/FFI Clippy. A later Plan shadow-reuse change passed both actual-media reuse/reload regressions and focused Clippy, with independent review. These results belong to their frozen local sources. The new local integration includes main `62c763b2`; its fresh full suite is pending. Plan session/DTO/caller integration and native Plan editing remain unfinished, and the new session remains compiled only for tests. Earlier native Page-editor and encrypted-checkpoint evidence stays tied to its original builds. The [Live Page contract](docs/product/live-pages.md) tracks the remaining workflows.

[PR 49](https://github.com/EnzoTironi/zoen-native/pull/49) merged after reviewed fixes and successful exact-head CI. [PR 51](https://github.com/EnzoTironi/zoen-native/pull/51) passed its Linux financial target, including 100 concurrent reservations, after 30 local real-service journeys. At `be659d9`, [PR 54](https://github.com/EnzoTironi/zoen-native/pull/54) passed 87 FFI tests, 24 gateway tests and both combined runtime targets in Linux CI. The complete run still failed inherited growth, claim and M2 targets. [PR 55](https://github.com/EnzoTironi/zoen-native/pull/55), `c1acf35`, publishes final native admission, lease-independent cancellation and financial closure gates. Both local runtime targets passed in 162.93 seconds, including all 26 new admission/cancellation outcomes and the retained financial regressions; four-package all-targets Clippy passed. Independent review verified the source, logs and executable hashes. Full exact-head Linux CI, guarded publication, production custody and external restore continuity remain open.

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
