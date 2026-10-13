# Zoen

Talk to people and agents in the same place. Keep the plans, pages, files and mini-apps produced by the conversation under your control.

© 2026 Enzo Tironi. All rights reserved. Public for viewing only. See [LICENSE](LICENSE).

Zoen is being built as a complete product for one billion monthly users. The shared Rust core powers native iPhone and Mac clients. Android and the reusable web shell are being integrated with the same conversation and document model.

## Current progress

Reviewed on 12 October 2026 against the verified product baseline [`ff5b519`](https://github.com/EnzoTironi/zoen-native/commit/ff5b519f1734e9d28018ab33d3919d12c9147e45). Native/web consolidation [PR 67](https://github.com/EnzoTironi/zoen-native/pull/67), bounded confirmed Plan history [PR 68](https://github.com/EnzoTironi/zoen-native/pull/68), keyboard controls [PR 69](https://github.com/EnzoTironi/zoen-native/pull/69) and native motion [PR 71](https://github.com/EnzoTironi/zoen-native/pull/71) are merged. PR 71's [complete Linux run](https://github.com/EnzoTironi/zoen-native/actions/runs/38309704131) passed **573 standard tests, zero failed or filtered, with one original cross-process helper ignored**, including all 198 actual FFI cases. Its tested, published and merged whole trees match. Custom log-store 10 and ownership 17 cases retain separate counts.

The merged native interface packages Core193. Its [reviewed images and screenshot walkthroughs](https://github.com/EnzoTironi/zoen-native/pull/67) cover exact Plan save/cold reopen/removal/restore, scoped Escape, themes, chat opening, Cards/List/Cards and actual TextKit typing, Unicode paste, undo and saved-version restoration. [PR 69's evidence](https://github.com/EnzoTironi/zoen-native/pull/69) verifies request review/back with focus return, change-event opening and quoted-message navigation. PR 71 adds three full arm64 Debug builds and bounded chat/thread/notification journeys. Its [settings evidence](https://github.com/EnzoTironi/zoen-native/pull/71#issuecomment-6132977105) also covers actual Reduce Motion, Reduce Transparency and Increase Contrast checks, with all temporary preferences restored.

The 12 October [shared-main checkpoint and complete handoff](docs/dev/handoff-2026-10-12.md) includes the current backend, Android, authenticated web and Apple work. Fresh checks pass 13 web behavior tests, 17 portable MLS tests, three web-Core Rust tests, seven attachment journeys, five push-boundary journeys and six Android verifier cases. Current full Core/CLI execution, generated bindings, browser pairing, Android device validation, native UI cleanup, push delivery, Release and full HIG remain open. The user requested pausing after publishing this checkpoint. New work uses one branch per unit. The coordinator merges that branch into `main` after the unit's proof, following the pstack playbooks.

| Area | Implemented and verified boundary | Remaining completion gates |
| --- | --- | --- |
| Messaging | Accounts, signed events, durable outbox, relay sync and OpenMLS encryption. Reviewed claim/M2 journeys retain the original 50-group workload. | Compatible production rollout, release journeys and sustained distributed capacity. |
| Devices and recovery | Enrollment/revocation, encrypted history transfer, backups and recovery after loss of every original MLS device. | Native recovery/device controls, current Android device proof and production backup authority. |
| Relay | Renewable ownership leases, transaction fencing, bounded forwarding and retry deduplication. | TLS workloads, cell placement, regional recovery, operating SLOs and complete costs. |
| Apple clients | PRs 67, 69 and 71 integrate unified Chats, shared themes, transparent overlays, pinned cards, notifications and keyboard controls. Scoped Core216 native builds also pass. | Current bounded media APIs, physical iOS, Release, complete accessibility/settings and remaining product workflows. |
| Android | [PR 41](https://github.com/EnzoTironi/zoen-native/pull/41) retains Android 28/35 and Rust CI. Current Plan and generated-ABI verification units are on shared main for integration. | Fresh bindings and current-main builds/device journeys, release and public relay compatibility. |
| Live Pages and Plans | Shared observed document session, encrypted drafts/retries/versions, typed Plan reads and common cards. Offline legacy edits, validated cache repair and private authenticated history prefixes are integrated. | Canonical Plan writers/history/approvals, complete block UX, comments/nested pages/live updates and two-device convergence. |
| Web | Themed shell and authenticated browser source; fresh typecheck/13 TS tests, 17 portable MLS tests and three web-Core Rust tests pass. | Real pairing/MLS messaging and reload/reconnect journeys, shared editor and native-equivalent accessibility/workflows. |
| Agents | Approval/authorization, financial claims and certified-runtime source are merged through PR 63. Scoped canonical decisions pass 216 local FFI cases, but PR 70's full hosted run failed a file-import journey. | Fix and verify that failure on current main; canonical writers, managed custody/continuity, productive model/tool/approval lifecycle and live provider reconciliation. |
| Media, push and shared resources | Encrypted blob, file, sandbox and infrastructure foundations. | Apple photo/voice delivery, APNs/FCM, remote MCP/catalog, community moderation, commerce and deployment/restore drills. |

Production and billion-user capacity require deployment, failure-recovery and cost evidence.

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

The certified runtime reads reply receipts in batches of at most 64 real history positions. A prefix retains every pending event, retry field, reference, and sealed publication byte for cold restore. Concurrent member messages can share an earlier verified `seen` without proving a fork. A signed `seen` that contradicts the held chain, or an authenticated checkpoint with an incompatible prefix or group digest, still parks sending and rejects cold activation. Restore also requires the exact certified agent device and owner bound to the image.

Run the receipt and fork regressions with:

```sh
cargo test --locked -j2 -p roda-ffi --lib runtime_core::tests::receipt_prefix_keeps_exact_pending_bytes_and_resumes_beyond_64_real_positions -- --exact --test-threads=1
cargo test --locked -j2 -p roda-ffi --lib runtime_core::tests -- --test-threads=1
cargo test --locked -j2 -p roda-ffi --lib mls::tests -- --test-threads=1
```
### Markdown import performance

`journey_files` measures import through relay acceptance of every sealed page. It exports every page on another device and compares the original bytes. The default hosted budget is 250 ms per page. The F5 measurement corpus had 94 UTF-8 Markdown files under `docs`, with 1,137,578 bytes in total. F7 adds documentation to the imported corpus; the aggregate candidate needs fresh corpus counts, hashes, timing and round-trip evidence.

Debug import spends most of its time on one CPU in Loro documents, message cryptography, and durable-queue decoding. `Cargo.toml` sets `opt-level = 3` for the sampled dependencies `loro-internal`, `loro-kv-store`, `sha2`, `chacha20`, `poly1305`, `serde_json`, `memchr`, and `foldhash`. The four core libraries `roda-docs`, `roda-log`, `roda-mls`, and `roda-store` have the same override because generic code uses the profile of the crate that instantiates it. [Cargo profile overrides](https://doc.rust-lang.org/cargo/reference/profiles.html#overrides-and-generics) explain that boundary. Debug assertions and overflow checks retain the development defaults. The application publish window is five messages per Space, matching the MLS receiver's out-of-order tolerance.

Compare five runs of `cargo test --locked -j2 -p zoen-cli --test journey_files -- --test-threads=1 --nocapture` on each revision. Keep the Rust toolchain, debug settings, two build jobs, disabled incremental compilation, and private Postgres and FoundationDB instances identical. Record the median and range of `IMPORT`, the successful byte count from `ROUND TRIP`, and compile cost from a separate `--timings` run. Profile CPU separately from reported timings. Keep the corpus unchanged between sides. These notes live in the root README so documentation does not add a page to that corpus.

## Documentation

Start with the [documentation index](docs/README.md), [current roadmap](docs/roadmap-status.md), [architecture](docs/architecture.md) and [system design](docs/system-design.md). The [Live Page contract](docs/product/live-pages.md), [session API](docs/dev/live-page-session.md) and [HIG matrix](docs/product/hig-compliance.md) separate implemented behavior from required acceptance. The [source auditor](tools/hig-audit/README.md) is advisory. [Dated history](docs/history/roadmap-2026-10-12-before-consolidation.md) preserves older results.

The product and built-in agent are Zoen. Technical `roda-*`, `RodaCore` and some preference/protocol identifiers still need the [atomic naming cutover](docs/dev/naming.md). Zoen has not launched: the [prelaunch policy](docs/dev/prelaunch-policy.md) requires direct caller/test updates and coordinated disposable database resets, without compatibility aliases or dual reads/writes.
