# Roadmap status: the path to one billion users

Evidence reviewed on 2026-10-09, against `main` at `17999988` and work in PRs
[#33](https://github.com/EnzoTironi/zoen-native/pull/33) through
[#44](https://github.com/EnzoTironi/zoen-native/pull/44).
GitHub records PRs 33, 34, 35, 36, 37 and 40 as merged. The examined `main` tree
does not contain PR 34's encrypted-backup implementation; reviewed PR 38 at `08124f0`
restores that integration and has [passing full Linux CI](https://github.com/EnzoTironi/zoen-native/actions/runs/37973200217).
The reviewed integration candidate `d9ebb3e`
has passing full Linux CI and remains a temporary, unmerged validation PR.
UI PR 44 at `836910c` has [passing full Linux CI](https://github.com/EnzoTironi/zoen-native/actions/runs/37980360708),
passing iOS/macOS builds and 13 web checks. Running Mac and browser journeys confirm
one Chats inbox for direct, group and community conversations, optional filters and
Cards/List notification switching. Full iOS interaction proof remains pending: recent
runs recorded render/accessibility stalls and a nonmoving scroll gesture. Computer-use
tooling subsequently detected the Mac was locked; further UI checks await unlock and
failure diagnosis. A temporary rendering treatment is not part of the reviewed head.
A controlled earlier AtTheEnd journey passed after deferring initialization of the
inactive Search screen.
Results are tied to named heads; earlier local M2 failures remain recorded below.
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
| Encryption (M2) | OpenMLS DMs/groups by default, membership removal, concurrent commit handling, sealed local state, reviewed pruning and explicit device enrollment; reviewed encrypted backup/recovery awaits restoration through PR 38 | Finish native linking/recovery UI and verify revocation and recovery through subsequent messages, including no surviving authorized MLS peer |
| Relay scale (S1–S9) | FDB ordering, dedupe, per-Space batching, NATS two-node fan-out, rate limits, telemetry, load harness | Shared leases and placement, crash-safe durable forwarding, cell routing, migration and regional failover proofs |
| Agent tools | Grants, signed tool manifests, budgets, sandbox lifecycle, gVisor/Firecracker/browser backends and merged WASM/Fly Machines providers | Real agent identities and MLS membership, durable model/tool run loop, approval resume, idempotent usage ledger, model gateway (M3); paid Fly staging and egress validation |
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

A separate user-owned backend workstream is implementing persisted renewable leases,
transaction fencing and bounded forwarding in [PR 43](https://github.com/EnzoTironi/zoen-native/pull/43).
Its owner reports 42 passing core tests, two cluster controls and seven client journeys
after fixing failures found by the first Linux run. Complete Linux CI and the isolated
ownership journey are still being checked. These are workstream reports, not a fresh
rerun by this audit, and do not close the production-cell or regional gates.

The analytics hot path already batches counters in RAM every 15 seconds.
Account/Space daily rows still accumulate in Postgres.
The design needs explicit memory bounds, retry/idempotency semantics, cell aggregation,
retention costs and reporting latency at the load model above.

## Work in flight

Six reviewed feature PRs are recorded as merged. The table distinguishes those integrations,
passing checks and the remaining native/UI proof. A green check or merge does not
establish product completeness.

| PR | Merge / reviewed head | Scope | Integration status / remaining proof |
|---|---|---|---|
| [33](https://github.com/EnzoTironi/zoen-native/pull/33) | Merged `551394c1` | Pruning, linking, enrollment, encrypted history | Integrated; native linking and repeatable complete M2 journeys remain gates |
| [34](https://github.com/EnzoTironi/zoen-native/pull/34) | GitHub records merge `2d39ab28` | Backup, authorized recovery enrollment, bounded storage | Missing from examined main tree; reviewed PR 38 restores it. Native recovery and recovery without a surviving authorized MLS peer remain gates |
| [35](https://github.com/EnzoTironi/zoen-native/pull/35) | Merged `7b3c548` | Real WASM sandbox | [Linux CI at that merge passes](https://github.com/EnzoTironi/zoen-native/actions/runs/37948425509) |
| [36](https://github.com/EnzoTironi/zoen-native/pull/36) | Merged `334ea050` | Fly Machines sandbox provider | Integrated; paid staging and production egress proof remain gates |
| [37](https://github.com/EnzoTironi/zoen-native/pull/37) | Merged `17999988` | E2B research | Integrated research; the live computer still needs implementation |
| [38](https://github.com/EnzoTironi/zoen-native/pull/38) | `08124f0` | Chat reading position, unread indicator and restored reviewed backup integration | [Full Linux CI passes](https://github.com/EnzoTironi/zoen-native/actions/runs/37973200217); combined native UI proof with PR 44 remains pending |
| [39](https://github.com/EnzoTironi/zoen-native/pull/39) | Documentation draft | Roadmap and all seven completion gates | Final statuses, attached chart and documentation-only diff after feature merges pending |
| [40](https://github.com/EnzoTironi/zoen-native/pull/40) | Merged `076d69e3` | Welcome ordering, refill, catch-up and stale-rejection fixes | Integrated; controlled regressions pass; historical stock-7 cause remains unproven and repeatability remains required |
| [41](https://github.com/EnzoTironi/zoen-native/pull/41) | Separate draft | Native Android client | Owned by another active chat; untouched by this audit; owner supplies live sync/media evidence |
| [42](https://github.com/EnzoTironi/zoen-native/pull/42) | `d9ebb3e` | Temporary combined validation PR | Full Linux CI passes; 41 local FFI tests and focused CLI/FFI Clippy pass; unmerged, close after original feature integration |
| [43](https://github.com/EnzoTironi/zoen-native/pull/43) | Separate backend draft | Renewable persisted bucket ownership, fencing and bounded forwarding | Owner reports core, cluster controls and client journeys passing after CI fixes; full Linux CI and isolated ownership proof pending |
| [44](https://github.com/EnzoTironi/zoen-native/pull/44) | `836910c` stacked on PR 38 | One Chats inbox, native header/pins/palette, Cards/List notifications and reusable web shell | Full Linux CI, iOS/macOS builds, 13 web checks and Mac/browser journeys pass; fresh iOS UI journeys pending. Web data remains a local sample preview |

Backup uses ADR 0046 and linking uses ADR 0045. Existing backup migration bytes were
preserved; later migrations extend generations, object versions and package idempotency.
The backup restoration must be verified in the final main tree. Native chat/UI proof and
the remaining full-product gates are separate from the backend merge records.

### Verified building blocks and unresolved failures

On `d9ebb3e`, **all 41 local FFI tests pass**, as do focused CLI and FFI Clippy checks.
Controlled regressions verify three client behaviors:

| Behavior | Old behavior | Fixed behavior |
|---|---|---|
| Initial catch-up before offline writes | Real-session test fails because a write overtakes catch-up | Queued plaintext and MLS maintenance wait for the first `SyncDone`; messages use the caught-up epoch and remain readable |
| Late stale rejection after incremental catch-up | Compiled socket regression stalls after 12.57 seconds | Rejection uses the epoch actually sent; exact newer ciphertext is retained, retried and read by the peer; a genuine current-epoch block remains intact |
| Slow maintenance and socket progress | Old interval fails the controlled notice test in 4 of 5 runs | Maintenance paced after each pass passes 5 of 5 runs |

The earlier `c7b1378` combined source has [passing full Linux CI](https://github.com/EnzoTironi/zoen-native/actions/runs/37953112924).
Its verification round also passed 85 local core tests (FFI 37, log 22, MLS 9, protocol 17),
19 backup journeys, 4 CLI revocation journeys, 1 connection journey, 3 key-package delivery
journeys, 3 wire journeys, 10 FoundationDB tests and 7 Postgres/socket authorization tests.
The newer combined `d9ebb3e` now also passes full Linux CI. The local results above remain
bound to their original verification rounds; CI does not replace release/native proof.

**The earlier local M2 round passed 8 of 10 journeys.** The offline-message journey left
a `ProfileKeyShared` acknowledgement pending after the user message reached recipient
history; the refill journey timed out earlier while adding Bruno to group 42. These local
failures remain part of the evidence even though that combined head passed Linux CI.
The new complete suites must establish repeatable recovery, delivery and refill.

[PR 40 CI at `fb5f822` failed live refill](https://github.com/EnzoTironi/zoen-native/actions/runs/37950442153):
stock 7, one Low notice enqueued, no client receipt, 15 pending and no rate limit.
The new controlled regressions prove specific ordering and scheduler hazards; they do
not establish the cause of that historical stock-7 failure. The reviewed backend PRs
are merged and combined Linux CI passes; repeatable release journeys, native proof and
the current UI verification remain required.

The candidate includes Welcome-before-message ordering, attaching a connecting device
before its stock query, notices ordered after successful package publication, and
idempotent package publication. The live-refill journey still requires exactly 32 packages
within its original 20-second polling window. Its stock-7 failure has not been waived.

### Device and recovery boundaries

[PR 33](https://github.com/EnzoTironi/zoen-native/pull/33) now requires explicit enrollment
for a new device on an existing account. Linking verifies the root certificate while an
active sponsor authorizes enrollment and box delivery in one Postgres transaction.
Protocol 4 refuses older clients before authentication so a version-3 sponsor cannot
report a successful link that leaves its target unenrolled. The upgrade preserves stored
history, local event metadata and the existing login signing domain; it does not require
wiping the database. See [ADR 0045](adr/0045-linking-devices.md).

Revocation rejects fresh requests and new deliveries for the revoked key. Work admitted
before the revocation commit can finish, including an already queued FoundationDB write;
Postgres revocation and the log append are separate transaction domains. The copied root
secret alone can no longer enroll another key at this relay. Valid backup recovery
authority can still authorize a fresh key, and keys enrolled before revocation stay
active until each is revoked. Those authorities need their own lifecycle. The durable
authorization checks and bounded Postgres pools also add costs that must be measured
under reconnect storms and sustained delivery before production-cell readiness.

Sealed v4 entries bind the surviving pruned header and use a separate signature domain
to prevent replay as a legacy stub. Full legacy v3 entries retain their authenticated
bytes and stay unpruned. Previously persisted legacy stubs authenticate their original
opaque digest; an upgrade cannot reconstruct trust in headers whose signed bytes were
already discarded. Existing chains and outboxes are preserved, rather than reset.

[PR 34](https://github.com/EnzoTironi/zoen-native/pull/34) adds generation-bound recovery
enrollment with a root certificate, proof from the new device and valid backup authority
in one transaction. Restore installs that same enrolled key and retains durable MLS join
requests; a surviving authorized peer must still add the leaf. Immutable object keys on
every upload attempt and an atomic pointer change preserve the previous backup when a
replacement fails. Database locks, object operations and full requests have bounded
waits; uncertain or failed cleanup still needs an operations-tested garbage collector.
See [ADR 0046](adr/0046-encrypted-backup.md).

**Production password recovery remains default off.** A vault key alone does not enable
it. The explicit development opt-in is for tests and development only. Public activation
requires independent, identity-bound recovery authorization before OPRF evaluation or
destructive counters, plus a real HSM/enclave implementation with proven key destruction
and counters across nodes. Removing a sealed key from Postgres does not destroy old
copies. Recovery-key mode remains available; native linking/recovery UI and recovery
without a surviving authorized MLS peer are still unfinished.

### Native and parallel work

Earlier [PR 38 native recordings](https://github.com/EnzoTironi/zoen-native/pull/38#issuecomment-6080457241)
cover three local timeline stories at `9ab80ca`. They were attached with `gh --attach`.
Five journeys now cover the expanded behavior. [Linux CI at `c836d5d3` passes](https://github.com/EnzoTironi/zoen-native/actions/runs/37963699446),
but the fresh native trial hung in the first fixture and was interrupted; it is not a
five-journey pass. Main-thread samples repeatedly passed through an invisible SwiftUI
native search controller. A focused `legacy.search` lazy-initialization gate is under
test, with its result still pending. Old recordings do not validate the changed source.
Simulator timeline stories also do not measure live relay delivery or production capacity.

The Grok computer also holds an unpublished `ux/audit-p2` patch across 14 Swift files.
The original patch remains saved and unimported; it still needs review, simulator
validation and a PR. The conversation confirms that some native flows are designed UI
awaiting their backing implementation, including the displayed agent browser. Track
these flows against their product completion gates.

[PR 41](https://github.com/EnzoTironi/zoen-native/pull/41) is the separate native Android
workstream. Its active chat owns validation and integration. A client implementation or
local planner does not by itself complete live cross-device delivery, production media
or the durable agent runtime.

Current UI work removes the global desktop top bar, shares the mobile floating chat
header, fixes the pin strip outside the timeline on a fully transparent background, and
extends content to the window top. The whole Mac app uses the iOS palette; the React web
shell uses the same palettes and side navigation. These changes are still undergoing
verification and need fresh visual evidence at their final source head. The independent
web shell/demo still has no authenticated web client. Live encrypted web sync,
onboarding, permissions and supported-version journeys remain part of gate 4.

## Execution order and acceptance gates

These are completion gates, not an MVP cut. Product and distributed infrastructure can
advance together once their common encrypted messaging boundary is reliable.

1. **Stabilize and integrate the existing work.** Close the live-refill failure, preserve
   unfinished UI changes, integrate reviewed linking/backup and sandbox work without ADR
   or migration collisions. Exit: combined-tree CI, release simulator journeys and attached
   visual evidence for each behavior; linking, history, recovery and revocation work through
   the native app, including the case with no surviving authorized MLS peer. Production
   password recovery also requires independent recovery authorization and proven key
   destruction before public activation.
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
   leases and fenced forwarding. Bound queues/caches and authorization work, test hot Spaces
   and reconnect storms, measure Postgres authorization capacity and backup cleanup, and
   isolate the load generator. Exit: agreed SLOs hold with TLS and MLS-sized payloads
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
