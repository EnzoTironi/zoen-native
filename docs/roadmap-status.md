# Roadmap status: the complete Zoen product

Reviewed on 10 October 2026. The target is one billion monthly users. The original prototype schedule does not define readiness.

## Version ledger

The integrated baseline is `main` at [`e051f97`](https://github.com/EnzoTironi/zoen-native/commit/e051f97f046ef8c98652cfe61edadd515f4b480d). The table distinguishes that baseline from changes published in parallel. Uncommitted changes remain work in progress.

| Workstream | Published version | Evidence and current state |
| --- | --- | --- |
| Integrated backend | PR 43, `050aa33`, merged as `893ab99`; PR 45, `075b835`, merged as `e051f97` | Renewable ownership, fencing, forwarding and encrypted/peerless recovery are in the actual main tree. [PR 45 full CI passed](https://github.com/EnzoTironi/zoen-native/actions/runs/38020174727). Post-merge [main CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38055889469) failed the live key-package refill journey. |
| Chat reading | [PR 38](https://github.com/EnzoTironi/zoen-native/pull/38), `07a04de` | Five reading journeys passed within the combined native UI verification. [Linux CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38058275281) failed: M1 offline reconnection reported `handle_taken`, and M2 final refill remained at 8 instead of 32. The new fixture separates stdout/stderr; it does not waive the stock-32 refill requirement. |
| Native shell and web preview | [PR 44](https://github.com/EnzoTironi/zoen-native/pull/44), `39ccb76`; native product source `50469b7` | Ten focused native journeys passed, with three additional visual-recording journeys. [Images and uncut Xcode videos](https://github.com/EnzoTironi/zoen-native/pull/44#issuecomment-6098239059) show Cards/List/Cards, pinned mini-app scroll/open/close and reading position. [CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38058379030) failed the final refill at 8 instead of 32. Web remains a sample-data preview. |
| Android integration | [PR 41](https://github.com/EnzoTironi/zoen-native/pull/41), `fdc492c` | Current main/recovery integration, canonical TLS clients, regenerated bindings and stronger restore/link/revoke tests are published. Local 66 FFI tests, focused Clippy and Kotlin compilation passed. [API 28/35 runtime CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38059052889) failed: API 35 completed 56 tests with one recovery assertion failure; API 28 failed the cold TLS fixture assertion. [Rust CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38059052906) failed the pre-refill stock check at 7 instead of 8. The owner has additional uncommitted UI work. |
| Durable agent proposals | [PR 46](https://github.com/EnzoTironi/zoen-native/pull/46), `54b9192` | Published atomic proposal storage and fixes for conflicting-device decisions, newer standing denials and grant-ID collisions. Fresh [CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38060358388) is running. The previous `f8a317c` CI passed; it does not certify this new version. Review and exact-head service proof remain required. |
| Temporary integration | [PR 42](https://github.com/EnzoTironi/zoen-native/pull/42), `d9ebb3e` | Historical combined validation only. Close after the original feature PRs integrate. It is not the current backend baseline. |

Before integrating a workstream, fetch its published head, compare production source and generated bindings, check results against that exact version, then inspect the resulting main tree. Earlier green runs do not certify later edits.

## Implemented foundations and unfinished gates

| Area | Implemented | Completion gate |
| --- | --- | --- |
| M1 accounts and sync | Device identities, signed binary events, durable outbox, paged sync and real relay/client journeys. | Release onboarding, interrupted sessions, account lifecycle and supported upgrades on each platform. |
| M2 encryption and recovery | OpenMLS direct/group encryption, pruning, explicit enrollment, revocation, paginated encrypted history, encrypted backups and peerless MLS recovery. PR 45 restored the backup implementation previously missing from main. | Native restore/link/revoke UI and subsequent-message proof, production password-vault authority, repeatable refill and enrollment/recovery failures. |
| S1–S9 relay | FoundationDB ordering/dedupe, persisted renewable bucket ownership, fencing, bounded routing, NATS fan-out, rate limits and telemetry. | Mixed TLS workloads, reconnect storms, authorization/lease capacity, placement, cell migration and regional failure recovery. |
| M3 agents | Grant evaluation, budgets, signed tools and sandbox providers; durable proposal/approval changes in review. | Authenticated MLS agent membership, durable model/tool loop, accepted-chain approval reconciliation, exactly-once usage ledger and live provider checks. |
| M4 media | Encrypted content-addressed blob transport, profile/background and file support; Android native media workflows under review. | Apple photo/voice delivery, interruption/resume and real object-store interoperability. |
| M5 push | Delivery design and infrastructure seams. | Token lifecycle, APNs/FCM workers, encrypted notification handling, retry/collapse behavior and physical-device proof. |
| Pages, files and mini-apps | Loro block editing, versions, file viewers, declarative views and sandboxed local mini-apps. | Notion-style block UX, nested pages and sharing, live bindings, comments/agent proposals, two-device editing and remote MCP/catalog installation. See the [live-page contract](product/live-pages.md). |
| Platforms | Native iOS/macOS; Android under review; themed web shell under review. | Authenticated browser client, encrypted browser storage and device enrollment, current ABI builds, native/web interoperability and distribution. |
| Communities and commerce | Conversation and permission foundations; unified Chats UI under review. | Discovery, moderation/reporting, shared-resource ACLs, business integrations, payment/credit ledger and provider reconciliation. |
| Growth | Invites, source attribution, experiments and content-free metrics. | Complete client acquisition flows, eligibility/abuse controls, bounded cell analytics and measured retention. |
| M7 operations | Infrastructure files, migrations, staging, health endpoints and telemetry. | Current compatible rollout, whole-stack deployment, alerts, admission, restore drills, regional recovery and cost reconciliation. |

## Verification boundaries

The native ten-journey run covers five reading cases, community navigation, Cards/List/Cards, fixed pinned-widget geometry and opening/closing its mini-app, plus chat/Home editing and pin persistence. These are UI journeys using an isolated demonstration account. They do not verify the newly integrated recovery cryptography.

The Android recovery integration compiled matching Kotlin bindings and passed 66 local native FFI tests. The local library was built for Mac ARM to validate shared core/binding metadata. Kotlin compilation skipped the Android ABI build. Android recovery/link/revoke and cold-process certificate rejection are not yet passing on the published version. API 35's recovery test reported password-backup availability after backup deletion; API 28's TLS fixture assertion needs exact exception diagnosis. Keep these failures visible while fixing them. Earlier Android media and UI evidence remains tied to its original source.

Main CI reproduced the unresolved live refill failure: a watching client reached `online`, `synced=true`, `pending=0`, and received a Low notice at stock 7, generated 25 packages and observed successful publication. The final assertion found 31 packages instead of 32. Its M2 suite passed 9 of 10 journeys. The new split-log fixture addresses a separate startup-observation failure; it does not establish the cause of the stock-7 failure. PRs 38 and 44 ended at stock 8 instead of 32 without a live Low notice or refill publication. PR 41 failed the earlier stock check at 7 instead of 8. These failures remain distinct from main's 31-versus-32 result and the earlier stock-7/no-client-Low failure. Durable claim retries and honest CLI completion are under investigation; no causal fix has passed yet. The stock-32 target and original 20-second polling window remain required.

Earlier controlled regressions proved catch-up before offline writes, preservation of newer ciphertext after a stale rejection and socket progress during slow maintenance. Earlier complete CI runs passed at their recorded versions. Those facts remain useful; the fresh refill failure still needs diagnosis and a causal regression.

The [previous audit](history/roadmap-2026-10-09.md) preserves the earlier failed local journeys and source-review history. Current states above supersede its PR and merge table.

## Production deployment

The public staging relay was last observed advertising protocol 2, while current shared clients require protocol 4. An `ok` health check does not establish compatibility. A reviewed rollout must inventory existing clients, apply compatible migrations, verify protocol negotiation and run fresh encrypted account/device journeys. An image rollback alone does not reverse migrations or newly written history.

Backup recovery-key mechanics and peerless MLS recovery are implemented. Password recovery still requires the production OPRF vault authority described in [ADR 0046](adr/0046-encrypted-backup.md). Development/test authority does not satisfy the hardware-backed production gate.

Revocation stops fresh requests and future deliveries. Work admitted before its commit can finish because PostgreSQL device authorization and FoundationDB log append are separate transaction domains. Recovered identities and independently enrolled devices need their own authority lifecycle. See [ADR 0045](adr/0045-linking-devices.md) and [ADR 0047](adr/0047-mls-peerless-recovery.md).

## Capacity evidence

The [load model](plan-real.md#scale-built-for-a-billion-people) assumes 500 million daily users, 700,000 peak messages/second, 3.5 million device deliveries/second and 160 million concurrent sockets. These are design inputs.

[ADR 0022](adr/0022-capacity.md) and [ADR 0023](adr/0023-per-space-sequencer.md) measured short development-box runs. The S9 sweep delivered 4,000 messages/second at p99 324 ms, above the 200 ms in-region target. One hot conversation reached about 3,180 messages/second. The 37 KiB/socket figure excludes TLS. The 150,000 sockets/node assumption and approximately $470,000/month projection do not establish production capacity or a bill; the cost projection excludes AI and media.

Persisted ownership now replaces process-local claims. Its bounded queues and renewable leases improve the implementation, but do not prove fleet renewal throughput, failover time, region loss recovery or the load model above.

## Required completion sequence

1. Integrate current reviewed native reading/shell work and resolve live refill without weakening its guarantee. Finish native recovery/linking and the current Android runtime checks.
2. Finish encrypted agent membership, model/tool execution, approval reconciliation, crash/retry behavior and usage accounting.
3. Complete encrypted photo/voice transport and push between independent devices.
4. Complete live pages, files, remote MCP/catalog, authenticated web, communities and commerce with permissions and cross-platform journeys.
5. Prove a production cell under mixed TLS workloads, hot conversations, reconnects and service failures. Reconcile offered, accepted and delivered work separately.
6. Prove cell placement/migration, compatible rolling upgrades, disaster recovery and regional failure behavior.
7. Validate capacity and full costs against the billion-user model with measured headroom, retention and operating SLOs.

External signing/APNs/provider/sandbox credentials activate their live checks. Independent local, protocol, simulator and failure testing can continue. Track each gate by its evidence instead of a completion percentage.
