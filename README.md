# Zoen

Talk to people and agents in the same place. Keep the plans, pages, files and mini-apps produced by the conversation under your control.

© 2026 Enzo Tironi. All rights reserved. Public for viewing only. See [LICENSE](LICENSE).

Zoen is being built as a complete product for one billion monthly users. The shared Rust core powers native iPhone and Mac clients. Android and the reusable web shell are being integrated with the same conversation and document model.

## Current progress

Reviewed on 12 October 2026 against `main` at [`a90bb52`](https://github.com/EnzoTironi/zoen-native/commit/a90bb52a1b588a91d32e4225f2a29cadc04f36b5). Native/web consolidation [PR 67](https://github.com/EnzoTironi/zoen-native/pull/67), bounded confirmed Plan history [PR 68](https://github.com/EnzoTironi/zoen-native/pull/68) and native keyboard controls [PR 69](https://github.com/EnzoTironi/zoen-native/pull/69) are merged over the backend/Page and offline compatibility work. PR 69's [complete Linux run](https://github.com/EnzoTironi/zoen-native/actions/runs/38302482067) passed **573 standard tests, zero failed or filtered, with one original cross-process helper ignored**, including all 198 actual FFI cases. Its tested, published and merged whole trees match. Custom log-store 10 and ownership 17 cases retain separate counts.

The merged native interface packages Core193. Its [reviewed images and screenshot walkthroughs](https://github.com/EnzoTironi/zoen-native/pull/67) cover exact Plan save/cold reopen/removal/restore, scoped Escape, themes, chat opening, Cards/List/Cards and actual TextKit typing, Unicode paste, undo and saved-version restoration. [PR 69's visual evidence](https://github.com/EnzoTironi/zoen-native/pull/69) verifies request review/back with focus return, change-event opening and quoted-message navigation. The motion follow-up at source `47c90fb` passes three full arm64 Debug app builds and three bounded Mac chat/thread/notification journeys, including a local thread reply retained after cold reopen. All 426 product and nine validation-source files remain literal after composing with Main 69. Actual Reduce Motion settings, current-core native packaging, Release, physical iOS, complete HIG, independent-device transport and production acceptance remain open.

| Area | Implemented and verified boundary | Remaining completion gates |
| --- | --- | --- |
| Messaging | Accounts, signed events, durable outbox, relay sync and OpenMLS encryption. Reviewed claim/M2 journeys retain the original 50-group workload. | Compatible production rollout, release journeys and sustained distributed capacity. |
| Devices and recovery | Enrollment/revocation, encrypted history transfer, backups and recovery after loss of every original MLS device. | Native recovery/device controls, current Android device proof and production backup authority. |
| Relay | Renewable ownership leases, transaction fencing, bounded forwarding and retry deduplication. | TLS workloads, cell placement, regional recovery, operating SLOs and complete costs. |
| Apple clients | PRs 67 and 69 integrate unified Chats, shared themes, transparent overlays, pinned cards, notifications and keyboard controls. The Core193 motion follow-up builds and bounded Mac chat/thread/notification journeys pass. | Current-core libraries/bindings, physical iOS, Release, complete accessibility/settings and remaining product workflows. |
| Android | [PR 41](https://github.com/EnzoTironi/zoen-native/pull/41), `e2a5406`, passes Android 28/35 and Rust CI. | Current-main integration, matched release/device journeys and public relay compatibility. |
| Live Pages and Plans | Shared observed document session, encrypted drafts/retries/versions, typed Plan reads and common cards. Offline legacy edits, validated cache repair and private authenticated history prefixes are integrated. | Canonical Plan writers/history/approvals, complete block UX, comments/nested pages/live updates and two-device convergence. |
| Web | Reusable themed shell and sample-data preview in `packages/zoen-ui`. | Authenticated messaging, encrypted storage/enrollment, shared editor and native-equivalent accessibility/workflows. |
| Agents | Approval/authorization, financial claims and certified-runtime source are incorporated through PR 63. A separate scoped canonical-decision unit passes all 216 local FFI cases after independent review; it is awaiting published-head integration. Nine superseded backend PRs are closed. | Canonical integration and public writers, managed custody/continuity, productive model/tool/approval lifecycle and live provider reconciliation. |
| Media, push and shared resources | Encrypted blob, file, sandbox and infrastructure foundations. | Apple photo/voice delivery, APNs/FCM, remote MCP/catalog, community moderation, commerce and deployment/restore drills. |

The [roadmap](docs/roadmap-status.md) records versions, evidence and completion gates. Production and billion-user capacity require their own deployment, failure-recovery and cost evidence.

## Product navigation

Chats contains direct conversations, groups and communities. Optional filters narrow it; community permissions and resources stay inside the conversation. Notifications open as cards, with explicit switches to a list and back. Plans use the same card and Live Page presentation as other items. Desktop and web follow the iOS themes and conversation hierarchy.

## Development

```sh
rustup toolchain install 1.99.0 --profile minimal --component rustfmt,clippy
export RUSTUP_TOOLCHAIN=1.99.0
cargo fmt --all --check
scripts/build-core.sh
```

Apple builds require Xcode, XcodeGen and the Rust Apple targets. Run `scripts/dev-stack.sh` in a separate terminal; it remains attached to the relay. The full Rust suite also requires PostgreSQL, FoundationDB, NATS and the sandbox providers configured by [CI](.github/workflows/ci.yml). Read [development setup](docs/dev/setup.md) before account/device journeys. The [web package](packages/zoen-ui/README.md) describes the reusable shell and preview.

## Documentation

Start with the [documentation index](docs/README.md), [current roadmap](docs/roadmap-status.md), [architecture](docs/architecture.md) and [system design](docs/system-design.md). The [Live Page contract](docs/product/live-pages.md), [session API](docs/dev/live-page-session.md) and [HIG matrix](docs/product/hig-compliance.md) separate implemented behavior from required acceptance. The [source auditor](tools/hig-audit/README.md) is advisory. [Dated history](docs/history/roadmap-2026-10-12-before-consolidation.md) preserves older results.

The product and built-in agent are Zoen. Technical `roda-*`, `RodaCore` and persisted preference/protocol identifiers remain for compatibility. The [naming migration](docs/dev/naming.md) tracks their replacement without losing account data or signed history.
