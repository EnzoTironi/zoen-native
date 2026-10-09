# Roadmap status: the path to one billion users

Evidence reviewed on 2026-10-09, against `main` at `f50b589` and work in PRs
[#33](https://github.com/EnzoTironi/zoen-native/pull/33) through
[#41](https://github.com/EnzoTironi/zoen-native/pull/41).
The target is the complete product at one billion monthly users.
The original prototype schedule in [repensado.md](repensado.md) does not determine readiness.

![Implementation, unfinished product work and scale completion gates](screens/roadmap-status.png)

## Current position

Zoen has real encrypted messaging foundations, a native Apple app, staging infrastructure,
and several tested building blocks for agents, files and growth. It is still completing
the product and its distributed infrastructure. A screen, an accepted ADR,
a passing unit test and a deployed journey are different kinds of evidence.

| Area | Implemented on main | Remaining completion gate |
|---|---|---|
| Accounts and sync (M1) | Device identities, signed binary events, durable outbox, paged sync, real relay and native account flow; [staging journey](../scripts/journey-staging.sh) | Repeatable release journeys across devices, interruptions and upgrades; account lifecycle and recovery UI |
| Encryption (M2) | OpenMLS DMs/groups by default, membership removal, concurrent commit handling, sealed local state and relay pruning | Integrate device linking and account backup, finish their native UI, verify revocation and recovery through subsequent messages |
| Relay scale (S1–S9) | FDB ordering, dedupe, per-Space batching, NATS two-node fan-out, rate limits, telemetry, load harness | Shared leases and placement, crash-safe durable forwarding, cell routing, migration and regional failover proofs |
| Agent tools | Grants, signed tool manifests, budgets, sandbox lifecycle, gVisor/Firecracker/browser backends | Real agent identities and MLS membership, durable model/tool run loop, approval resume, idempotent usage ledger, model gateway (M3) |
| Media (M4) | Encrypted blob transport, backgrounds, profile photos and Item blobs | Photos and voice messages through the same transport, interrupted/resumed transfer, real S3 integration journey |
| Push (M5) | Planned protocol and delivery design | Token registration, APNs worker, encrypted notification extension, simulator and HTTP/2 sink journeys, then physical-device validation |
| Files and apps | Loro documents, file viewers/editors, pages, declarative view types and local mini-app examples (ADRs 0040–0042) | Multi-device editing and agent approval journeys, complete permissions, remote MCP interoperability, signed catalog publishing/search/install (M6) |
| Growth and analytics | Invite mechanics, MAU/sent-message counters, retention reports, source attribution, experiment assignment/exposure foundations (ADRs 0043–0044) | Client flows per acquisition source, experiment guardrails, stronger invite eligibility, partitioned/columnar analytics and validated cell-level aggregation |
| Platforms and distribution | Native iOS/macOS; real staging in Fly gru behind Cloudflare | Android and functional web client, universal links/passkeys, notification entitlements, release and compatibility testing |
| Communities and commerce | Product design and UI foundations | Real discovery, moderation/reporting, creator/community workflows, business agent integrations and payment/credit ledger |
| Operations (M7) | Infrastructure files, migrations, local relay stack, health endpoints and telemetry | Whole-stack deployment including agents/push, restore drills, SLO alerts, capacity admission, chaos and regional recovery drills |

Main evidence: [plan-real.md](plan-real.md), [system-design.md](system-design.md),
[product vision](product/vision.md), [UX audit](product/ux-audit.md), and the named journey
tests under `crates/zoen-cli/tests` and `apple/UITests`. Rows describe implementation,
not a fresh rerun of every historical test.

## What the scale evidence proves

The [load model](plan-real.md#scale-built-for-a-billion-people) assumes 500 million daily
users, 700,000 peak messages/second, 3.5 million peak device deliveries/second and 160
million concurrent sockets. These are design inputs.

[ADR 0022](adr/0022-capacity.md) and [ADR 0023](adr/0023-per-space-sequencer.md) measure
a shared development box over short runs. The S9 sweep delivers 4,000 messages/second
with p99 324 ms, above the system design's in-region 200 ms delivery goal. One hot Space
tops out near 3,180 messages/second. The 37 KiB/socket measurement excludes TLS; 150,000
sockets per node is a planning assumption. The approximately $470,000/month estimate is
an extrapolation that excludes media and AI, not a production bill or capacity proof.

`NodeOwner::claim_all` in `crates/zoen-relay/src/ownership/mod.rs` grants all 4,096
partitions to each process using an in-memory lease table and a one-year expiry.
FDB head conflicts protect ordering across racing relays; there is no shared ownership
handoff yet. The system design's two-second lease failover remains a target.

The analytics hot path already batches counters in RAM every 15 seconds, rather than
writing a SQL row per message. Account/Space daily rows still accumulate in Postgres.
The design needs explicit memory bounds, retry/idempotency semantics, cell aggregation,
retention costs and reporting latency at the load model above.

## Work in flight

This table records the audit's latest evidence. Follow each PR's current checks before
integrating it; a green check alone does not establish product completeness.

| PR | Scope | Validation / remaining integration gate |
|---|---|---|
| [33](https://github.com/EnzoTironi/zoen-native/pull/33) | Pruning ceiling, stale-device rejoin, linking, paginated encrypted history | Ten M2 journeys pass locally; both full Linux CI runs pass at `6d08daa`. Earlier live-refill failures and native linking UI remain integration gates |
| [34](https://github.com/EnzoTironi/zoen-native/pull/34) | Encrypted password/recovery-key backup | Full Linux CI passes at `d5efc69`; six backup journeys and ten M2 journeys pass locally on the combined tree. Native recovery UI and no-surviving-device protocol remain |
| [35](https://github.com/EnzoTironi/zoen-native/pull/35) | Real WASM sandbox | CI passes; integrate with the runtime after manifest/grant review |
| [36](https://github.com/EnzoTironi/zoen-native/pull/36) | Fly Machines sandbox provider | CI passes; paid staging deployment and production egress policy still need their own proof |
| [37](https://github.com/EnzoTironi/zoen-native/pull/37) | E2B research | CI passes; research informs design and does not implement the proposed live computer |
| [38](https://github.com/EnzoTironi/zoen-native/pull/38) | Chat reading position and unread indicator | Three native XCTest journeys pass at `9ab80ca`; video and screenshots are attached. Full Linux CI passes at `991b2fa`; its native source matches the verified head |
| [39](https://github.com/EnzoTironi/zoen-native/pull/39) | Evidence-backed roadmap and completion gates | Documentation diff on PR 40. CI at `9678d38` failed live refill; diagnostic additions are covered by its current checks |
| [40](https://github.com/EnzoTironi/zoen-native/pull/40) | Welcome ordering and connection-time refill | Controlled before/after regressions and full Linux CI pass at `ccb18c6`. Isolated refill and Clippy pass with diagnostics at `46807c1`; the intermittent failure remains an investigation |
| [41](https://github.com/EnzoTironi/zoen-native/pull/41) | Native Android client | Rust and native CI pass. Live two-device relay sync and production encrypted media still need their own journey evidence |

The initial linking and backup heads both used ADR number 0045. Backup now uses 0046
in PR 34; its existing migration bytes were preserved. Verify migration numbering and
recovery behavior against the combined linking/backup tree before integration.

### Reliability work from this audit

[PR 40](https://github.com/EnzoTironi/zoen-native/pull/40) repairs two MLS delivery boundaries.
A sender now waits for its queued Welcome to be confirmed before publishing application
messages. A connecting device is attached to live delivery before the relay reads its
key-package stock, so a concurrent claim cannot fall between that snapshot and mailbox
attachment. Controlled WebSocket and NATS journeys fail on the original code and pass
with their respective fixes. The rate-limit journeys and existing refill journey pass
locally; production quotas and the refill target remain unchanged. Follow the PR's
[current workspace checks](https://github.com/EnzoTironi/zoen-native/pull/40/checks).

The repair is included in PRs 33, 34 and 38. PR 33's stale-device fixture keeps active peers
checkpointing throughout the absent device's pruning window. Further diagnostics confirmed
that the CLI's watching banner can appear after a catch-up timeout. The refill fixture now
waits for the watcher's latest online, synced state and verifies that it stays alive. It
still requires exactly 32 packages within the original 20-second polling window. The
offline-message fixture also drains the sender's initial Welcome before taking the
recipient offline through subsequent commits.

A subsequent PR 33 CI run still failed live refill with seven packages after the watcher
reported online and synced. Its other run at the same head passed. The failure recurred
on [PR 39 at `9678d38`](https://github.com/EnzoTironi/zoen-native/actions/runs/37929949846).
The diagnostic heads capture low-stock receipt, generated batches, publication replies
and session metrics before watcher cleanup. PR 40 also records relay notice delivery
counts and warns when its stock query fails. Ten M2 journeys pass locally on the recovery
tree; an isolated refill journey passes on the messaging tree with both offline and live
notices followed by successful publication. These are diagnostic results, not a root-cause
fix. This intermittent failure remains an integration gate, even when a run passes.

[PR 38's native evidence](https://github.com/EnzoTironi/zoen-native/pull/38#issuecomment-6080457241)
covers the reading anchor during two arrivals, the capsule jump, automatic following at
the bottom, and opening at the first unread boundary. All three XCTest journeys pass on
an iPhone 17 Pro / iOS 27.0 arm64 Simulator. The recording and screenshots were attached
with `gh --attach`. These are local stories using the native timeline; they do not measure
live relay delivery or production capacity.

[PR 34](https://github.com/EnzoTironi/zoen-native/pull/34) now builds on PR 33. Restore
persists the new device's signed MLS join requests in the durable outbox. Both password
and recovery-key journeys exchange new encrypted messages through a surviving group
admin, revoke the lost phone and verify signed history. A stale backup cannot regain
removed membership. The combined recovery and M2 journeys pass locally. Native recovery
and linking UI, release device journeys, and recovery without a surviving authorized MLS
device remain completion gates; see the
[restore design](https://github.com/EnzoTironi/zoen-native/blob/feat/encrypted-backup/docs/adr/0046-encrypted-backup.md).

The Grok computer also holds an unpublished `ux/audit-p2` patch across 14 Swift files.
It was saved before further work; it still needs review, simulator validation and a PR.
The conversation confirms that some native flows are designed UI awaiting their backing
implementation, including the displayed agent browser. Track these flows against their
product completion gates.

[PR 41](https://github.com/EnzoTironi/zoen-native/pull/41) now adds a native Kotlin/Compose
Android client backed by the shared Rust engine. Its verification record reports native
UI, Keystore, persistence and release-build checks. Review its own CI and
[remaining coverage](https://github.com/EnzoTironi/zoen-native/blob/codex/native-android/docs/dev/android.md):
live two-device relay sync and production encrypted media transfer still need proof, and
the local planner does not complete the durable agent runtime.

## Execution order and acceptance gates

These are completion gates, not an MVP cut. Product and distributed infrastructure can
advance together once their common encrypted messaging boundary is reliable.

1. **Stabilize and integrate the existing work.** Diagnose the failed M2 journeys, preserve
   unfinished UI changes, integrate linking/backup and sandbox work without ADR or migration
   collisions. Exit: combined-tree CI, release simulator journeys and attached visual evidence
   for each behavior; linking, history, recovery and revocation work through the native app.
2. **Make the agent a real participant.** Build the approved Rust/FDB/JetStream runtime,
   keeping the model provider behind Zoen-owned traits. Exit: an MLS group member asks the
   agent, receives its encrypted reply, approves an otherwise refused tool, and sees one
   usage debit even after a worker crash/retry. Prove it with a scripted model endpoint,
   then a configured live provider.
3. **Complete daily communication.** Finish encrypted photo/voice delivery, resumable
   transfers and push. Exit: a recipient with the app closed receives a notification,
   opens the message/media, survives a network interruption and remains in sync; storage,
   pushes and telemetry contain no message plaintext.
4. **Complete product reach.** Finish signed catalog/MCP installation, cross-device files,
   Android/web onboarding and invite links, community moderation and business workflows.
   Exit: each promised flow works between independent accounts and platforms with scoped
   permissions, accessibility and supported client versions.
5. **Prove a production cell.** Replace process-local ownership with shared, renewable
   leases and fenced forwarding. Bound queues/caches, test hot Spaces and reconnect storms,
   and isolate the load generator. Exit: agreed SLOs hold with TLS and MLS-sized payloads
   during a soak, a relay/NATS/storage failure, and restore from backup. Record offered,
   accepted and delivered work separately with exact reconciliation.
6. **Prove cells and regions.** Implement directory/cell placement and migration, DR and
   version-compatible rolling changes. Exit: concurrent writers, duplicate delivery,
   network partitions and region loss preserve ordering, membership and recoverable
   messages with measured recovery objectives.
7. **Validate the billion-user operating model.** Aggregate analytics by cell, run
   representative mixed workloads and measure full costs including media, model use and
   replication. Exit: demonstrated cell capacity multiplied by tested placement/failover
   limits satisfies the load model with stated headroom; growth experiments preserve
   retention and operational SLOs. Keep modeled, measured and deployed numbers distinct.

## External activation

Apple signing capabilities/APNs credentials, a live model provider and paid sandbox
deployment activate their respective live checks. Their absence does not prevent local
protocol, runtime, simulator or failure testing. GitHub Actions is currently running:
the old private-repository billing blocker in the plan is stale.

The ongoing product metrics remain **MAU and sent messages per user**, interpreted with
retention, delivery success, privacy and cost. A completion percentage would hide the
largest unproven gates; track their evidence instead.
