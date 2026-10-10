# Zoen

Talk to people and agents in the same place. Keep the plans, pages, files and mini-apps produced by the conversation under your control.

© 2026 Enzo Tironi. All rights reserved. Public for viewing only. See [LICENSE](LICENSE).

Zoen is being built as a complete product for one billion monthly users. The shared Rust core powers native iPhone and Mac clients. Native Android is under review in [PR 41](https://github.com/EnzoTironi/zoen-native/pull/41). The web shell is under review in [PR 44](https://github.com/EnzoTironi/zoen-native/pull/44); authenticated browser messaging remains unfinished.

## Current progress

This snapshot uses `main` at `e051f97`, the merge of encrypted recovery [PR 45](https://github.com/EnzoTironi/zoen-native/pull/45), on 10 October 2026. Work published in parallel remains identified by its own commit and PR.

| Area | Current implementation and evidence | Still required |
| --- | --- | --- |
| Messaging | Real accounts, signed events, durable outbox, relay sync and OpenMLS encryption for direct and group conversations. A local durable key-package claim candidate passed its causal reply-loss journey. | The unchanged 50-group refill journey, exact-main CLI baseline, repeatable release journeys and supported-client upgrades. |
| Devices and recovery | Explicit enrollment, revocation, encrypted history transfer, encrypted backups and recovery after loss of every original MLS device. Backend changes are integrated. | Native recovery and device-management UI, fresh Android runtime proof, production backup authority. |
| Relay | Persisted renewable ownership leases, transaction fencing, bounded forwarding and retry deduplication from [PR 43](https://github.com/EnzoTironi/zoen-native/pull/43). | Production TLS workloads, cell placement, regional recovery, sustained capacity and full cost evidence. |
| iOS and Mac | Native clients and shared themes. [PRs 38](https://github.com/EnzoTironi/zoen-native/pull/38) and [44](https://github.com/EnzoTironi/zoen-native/pull/44) add one Chats inbox, reading-position preservation, pinned widgets and Cards/List notifications. Ten focused iOS journeys passed on the combined UI source. | Fresh checks on the integration heads, release/device coverage and the remaining product flows. |
| Android | Kotlin/Compose client with shared Rust bindings, native Keystore, encrypted relay/media journeys and adaptive layouts in PR 41, now at `339b5f1`. | The fresh API 28 check failed; API 35 and Rust checks are running. Verify the current readiness/UI changes and retain the older device-failure evidence by its source version. |
| Pages and mini-apps | Loro block documents, native rich-text editing, versions, file viewers and sandboxed mini-app hosts. A local editing candidate adds encrypted draft receipts and retry/restore fencing. | Native editor runtime proof, complete Notion-style page workflows, live bindings, collaborative/agent updates and authenticated web parity. |
| Agents | Grants, budgets, approvals and signed tool/sandbox foundations. Durable proposals ([PR 46](https://github.com/EnzoTironi/zoen-native/pull/46)) and enrolled-owner-device registration ([PR 48](https://github.com/EnzoTironi/zoen-native/pull/48)) are under review. | Green exact-head service checks, encrypted agent membership/runtime, crash-safe model/tool execution, approval reconciliation and provider billing. |
| Communication and operations | Encrypted blob foundations, infrastructure, migrations and telemetry. | Apple photo/voice transport, push, community moderation, catalog/commerce and tested deployment/restore procedures. |

The [roadmap](docs/roadmap-status.md) records exact versions, failures and completion gates. Current Android, agent and documentation CI runs have failed; the ledger distinguishes their assertions from main's 31-versus-32 refill result. The claim candidate in [PR 50](https://github.com/EnzoTironi/zoen-native/pull/50) at `dd03f6e` passed the unchanged 50-group journey in 180.07 seconds and all nine causal claim journeys in 26.42 seconds; workspace Clippy also passed. Its exact-head CI failed growth reporting, two claim fixtures and the 36th group's settlement; those failures are being corrected without weakening the assertions. The local Pages candidate passed 13 document tests, all 80 FFI tests and 95 Gradle unit tests across 15 suites. Android ABI/device execution and current native editor runtime proof remain pending. Its [behavior contract](docs/product/live-pages.md) keeps comments, nested pages, sharing, live bindings and guarded agent tools as unfinished work.

The committed Mac candidate built successfully and produced app bundles. A subsequent rebuild restricts C/C++ objects to macOS 26: all 1,152 archive objects declare a minimum of macOS 26 or earlier, and the Mac app rebuilt successfully without the newer-target warnings. Runtime on macOS 26 remains unverified. The Reader/history/debug-isolation follow-up built successfully. A manual Mac journey verified editing, visible version history and restoration as a new version. The encrypted Apple keystroke checkpoint built on Mac and passed ten recovery journeys, including a recreated native development vault, isolated unreadable-draft discard and refusal of a stale discard confirmation. The actual Mac re-open/history XCTest recording is in progress. Release Keychain, native composition callbacks and iOS file protection still require runtime proof. The parallel [model gateway](https://github.com/EnzoTironi/zoen-native/pull/49) passed exact-head CI. Source review found two correctness gaps in accepted tool outputs and partial usage receipts; fixes and fresh checks are required before integration. Durable agent runtime integration remains unfinished.

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
