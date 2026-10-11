# Zoen

Talk to people and agents in the same place. Keep the plans, pages, files and mini-apps produced by the conversation under your control.

© 2026 Enzo Tironi. All rights reserved. Public for viewing only. See [LICENSE](LICENSE).

Zoen is being built as a complete product for one billion monthly users. The shared Rust core powers native iPhone and Mac clients. Native Android is under review in [PR 41](https://github.com/EnzoTironi/zoen-native/pull/41). The web shell is under review in [PR 44](https://github.com/EnzoTironi/zoen-native/pull/44); authenticated browser messaging remains unfinished.

## Current progress

This snapshot uses `main` at [`f7b8744`](https://github.com/EnzoTironi/zoen-native/commit/f7b8744b2968a019818b35e3309573292de3e970), including the shared Page session from [PR 60](https://github.com/EnzoTironi/zoen-native/pull/60) and documentation update 61, on 11 October 2026. The Plan consumer candidate below remains separate from merged main. Parallel work is identified by its own commit and checks.

| Area | Current implementation and evidence | Still required |
| --- | --- | --- |
| Messaging | Real accounts, signed events, durable outbox, relay sync and OpenMLS encryption. PR 50 is merged; fresh Linux CI passed all nine claim journeys and all ten M2 journeys, including the unchanged 50-group refill. | Production relay migration, supported-client upgrades and repeatable release journeys. |
| Devices and recovery | Explicit enrollment, revocation, encrypted history transfer, encrypted backups and recovery after loss of every original MLS device. Backend changes are integrated. | Native recovery and device-management UI, fresh Android runtime proof, production backup authority. |
| Relay | Persisted renewable ownership leases, transaction fencing, bounded forwarding and retry deduplication from [PR 43](https://github.com/EnzoTironi/zoen-native/pull/43). | Production TLS workloads, cell placement, regional recovery, sustained capacity and full cost evidence. |
| iOS and Mac | Native clients and shared themes. [PRs 38](https://github.com/EnzoTironi/zoen-native/pull/38) and [44](https://github.com/EnzoTironi/zoen-native/pull/44) add one Chats inbox, reading-position preservation, pinned widgets and Cards/List notifications. Full Linux checks pass at both published heads; ten earlier focused iOS journeys retain their original build scope. | Current-main integration, fresh native runtime/HIG journeys, release/device coverage and remaining product flows. |
| Android | Native Kotlin/Compose client with shared Rust bindings, Keystore, encrypted media and adaptive layouts. At `e2a5406`, [PR 41](https://github.com/EnzoTironi/zoen-native/pull/41) passes Android 28, Android 35 and Rust CI. | Integration with current main, source-matched release/device evidence, public relay compatibility and remaining provider workflows. |
| Pages and mini-apps | PR 60 is merged: one Binary Page session for observed editing, encrypted recovery, exact retries, versions and atomic publication. Its [Linux CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38156415209) passed 468 tests across 65 targets. Matching bindings compiled in Mac and ARM64 iOS Simulator apps. The local Plan consumer candidate passes 127 FFI tests: cards, details and search use accepted Plan content and exclude private drafts. PR 44 gives plans the pinned mini-app card pattern. | Fresh complete Linux checks for the consumer candidate, native editor UI validation, typed Plan clients/history/writers and C4 approval reconciliation, complete Notion-style/two-device workflows, live bindings and authenticated web parity. |
| Agents | Grants, budgets, approvals, model gateway and sandbox foundations. The financial/certified-run stack through [PR 58](https://github.com/EnzoTironi/zoen-native/pull/58), `d4d7ae4`, remains under review. Its local output-boundary run passed all ten C4 and thirty financial scenarios with the same signed grant and a 1,692 ms native-limited window. [Fresh Linux CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38159973159) passed 422 tests, with one existing helper ignored. The earlier `5f380a2` Linux failure remains preserved. | Current-main integration, authenticated managed custody, restore continuity, exclusion of old writers, guarded publication, driver lifecycle and live provider reconciliation. |
| Communication and operations | Encrypted blob foundations, infrastructure, migrations and telemetry. | Apple photo/voice transport, push, community moderation, catalog/commerce and tested deployment/restore procedures. |

The [roadmap](docs/roadmap-status.md) records exact versions and completion gates. [PR 50's final Linux run](https://github.com/EnzoTironi/zoen-native/actions/runs/38113755244) passed formatting, workspace Clippy, 60 FFI tests, nine causal claim journeys, ten M2 journeys and the metrics target. Its tested tree matches merged main. The original eight-second CLI deadline, 50-group workload, low-water threshold of 8, stock target of 32 and five-second claim backoff are unchanged. Earlier diagnostic and failed runs remain historical evidence; they do not describe the merged candidate. This pass does not establish production deployment or billion-user capacity.

The fresh main Linux run tests PR 60’s merged tree: 61 document, 7 compatibility and 120 FFI tests passed; the complete run has 468 passing tests, zero failures and one existing cross-process helper marked ignored. A separate native editor candidate compiled on Mac and ARM64 iOS Simulator and passed eight Unicode and ten encrypted checkpoint/recovery journeys. Its corrected history UI still needs actual runtime confirmation. Plan writers/clients, authenticated browser editing and complete HIG acceptance remain open. The [Live Page contract](docs/product/live-pages.md) distinguishes the merged core from those workflows.

The Plan consumer candidate at `610895f` passed formatting, all-target FFI Clippy and all 127 FFI tests, with no ignored or filtered tests. The seven new tests reproduced six failures against unchanged main production code before the fix. Refused older writes remain in history; legacy mutations refuse before changing storage after cutover, and exact text, duplicate IDs and integer cents are preserved. Public Plan Page/edit APIs remain closed. See the [session contract](docs/dev/live-page-session.md) for compatibility and total-overflow behavior.

The backend’s local test explicitly uses a 20-second preparation grant, issued once before the original trigger/model call, and waits after staging until the same signed grant has at most 1.7 seconds remaining. The known SQL commit then has 1,692 ms remaining; the delayed acknowledgement is refused exactly. Fresh Linux CI passed the same published source, but its successful runtime log does not emit individual case markers or the probe value. The older failed Linux run remains scoped to `5f380a2`. Composition with the shared Page core initially passed 157 FFI tests and failed eleven; the admission correction is source-reviewed and awaits compilation and the complete suite. Public native output remains closed. The [roadmap ledger](docs/roadmap-status.md) records the current head and historical evidence.

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
