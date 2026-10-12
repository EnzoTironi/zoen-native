# Zoen

Talk to people and agents in the same place. Keep the plans, pages, files and mini-apps produced by the conversation under your control.

© 2026 Enzo Tironi. All rights reserved. Public for viewing only. See [LICENSE](LICENSE).

Zoen is being built as a complete product for one billion monthly users. The shared Rust core powers native iPhone and Mac clients. Android and the reusable web shell are being integrated with the same conversation and document model.

## Current progress

Reviewed on 12 October 2026 against `main` at [`3c8cef9`](https://github.com/EnzoTironi/zoen-native/commit/3c8cef986a0d4e13e2772fcfdc28e1607a2f0247). Backend/Page composition [PR 63](https://github.com/EnzoTironi/zoen-native/pull/63) and offline Plan compatibility [PR 66](https://github.com/EnzoTironi/zoen-native/pull/66) are merged. PR 66's [complete Linux run](https://github.com/EnzoTironi/zoen-native/actions/runs/38288186205) tested the exact resulting main tree: **568 passed, zero failed, one original cross-process helper ignored**, including all 193 actual FFI cases. Custom log-store and ownership harnesses retain their separate counts.

The current native candidate integrates that main. Matching Core193 libraries and full arm64 Mac, iOS simulator and generic iOS device Debug apps compile. Eleven Swift/API cases pass. Actual Mac journeys at `eec5c432`, on macOS 27.2 with an owned demo database, verify Plan save/cold reopen, exact cents, removal/restoration, scoped Escape, themes, chat opening and Cards/List/Cards. [Images and the screenshot walkthrough](https://github.com/EnzoTironi/zoen-native/pull/66#issuecomment-6130391154) retain that client scope. Release, physical iOS, complete HIG, independent-device transport and production acceptance remain open.

| Area | Implemented and verified boundary | Remaining completion gates |
| --- | --- | --- |
| Messaging | Accounts, signed events, durable outbox, relay sync and OpenMLS encryption. Reviewed claim/M2 journeys retain the original 50-group workload. | Compatible production rollout, release journeys and sustained distributed capacity. |
| Devices and recovery | Enrollment/revocation, encrypted history transfer, backups and recovery after loss of every original MLS device. | Native recovery/device controls, current Android device proof and production backup authority. |
| Relay | Renewable ownership leases, transaction fencing, bounded forwarding and retry deduplication. | TLS workloads, cell placement, regional recovery, operating SLOs and complete costs. |
| Apple clients | Unified Chats, shared themes, transparent chat overlays, pinned cards and notification views in the current native candidate. Core193 builds and bounded Mac runtime journeys pass. | Integrate the published native candidate; physical iOS, Release, accessibility and remaining product workflows. |
| Android | [PR 41](https://github.com/EnzoTironi/zoen-native/pull/41), `e2a5406`, passes Android 28/35 and Rust CI. | Current-main integration, matched release/device journeys and public relay compatibility. |
| Live Pages and Plans | Shared observed document session, encrypted drafts/retries/versions, typed Plan reads and accepted card/search projections. PR 66 preserves successive offline legacy edits and repairs validated damaged-media imports. | Canonical Plan writers/history/approvals, complete block UX, comments/nested pages/live updates and two-device convergence. |
| Web | Reusable themed shell and sample-data preview in `packages/zoen-ui`. | Authenticated messaging, encrypted storage/enrollment, shared editor and native-equivalent accessibility/workflows. |
| Agents | Approval/authorization, financial claims and certified-runtime source are incorporated through PR 63; their bounded fixtures retain separate evidence. Nine superseded backend PRs are closed. | Managed custody/continuity, productive model/tool/approval lifecycle, strict canonical replay and live provider reconciliation. |
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
