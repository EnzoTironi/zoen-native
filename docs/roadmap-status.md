# Roadmap status: the complete Zoen product

Reviewed on 11 October 2026; published-head snapshot at 17:16 UTC. The target is one billion monthly users. Readiness requires implemented workflows, current client journeys and production evidence.

## Version ledger

The integrated baseline is `main` at [`fa4eed8`](https://github.com/EnzoTironi/zoen-native/commit/fa4eed802b1afc96dd56d9f2365a5ccf30d1b933), including PR 60. Its tree equals the tested PR 60 tree. Parallel heads below still require their own integration and validation.

| Workstream | Published version | Evidence and current state |
| --- | --- | --- |
| Integrated backend and documentation | PRs 43, 45, 49, 47, 50 and 59 merged before PR 60 | Ownership/fencing, encrypted recovery, model adapter, durable claim retries and reviewed documentation are integrated. PR 50's [full Linux run](https://github.com/EnzoTironi/zoen-native/actions/runs/38113755244) passed the unchanged workload. Relay migration 0024, compatible client upgrades and production account journeys remain required. |
| Shared Page session | [PR 60](https://github.com/EnzoTironi/zoen-native/pull/60), `b9aafd0`, merged as `fa4eed8` | [Full Linux CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38156415209) passed 468 tests across 65 targets, including 61 document, 7 compatibility and 120 FFI tests; zero failures and one existing cross-process helper ignored. Both the PR run and the fresh main push run test the merged tree. Observed editing, encrypted recovery, exact retries, restore fencing, versions and atomic publication share one Binary Page session. Plan Item APIs remain closed. Matching bindings compiled in Mac and ARM64 iOS Simulator apps. |
| Chat reading | [PR 38](https://github.com/EnzoTironi/zoen-native/pull/38), `7a22527b`, including older main `62c763b2` | [Current full Linux CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38116958003) passed all 65 targets at the published tree. The five earlier native reading journeys retain their original executable scope. Fresh native journeys and integration with current main `fa4eed8` remain required. |
| Native shell and web preview | [PR 44](https://github.com/EnzoTironi/zoen-native/pull/44), `2963b246`, draft on PR 38 | [Current full Linux CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38117040162/attempts/2) passed all 65 targets after an ordinary retry of a service-container startup failure. Apple/web UI bytes remain those of `27245db`; [actual Mac images/video](https://github.com/EnzoTironi/zoen-native/pull/44#issuecomment-6104071096) retain that older source scope. Authenticated web messaging, current-main native journeys and full HIG acceptance remain open. |
| Android integration | [PR 41](https://github.com/EnzoTironi/zoen-native/pull/41), `e2a5406`, draft | [Android 28/35 CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38112132473) and [Rust CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38112132479) pass. Physical S23+ proof belongs to the earlier `302b2f0` build. Current-main integration, matched release/device journeys and public relay compatibility remain required. |
| Financial and certified agent runtime | Stack through [PR 58](https://github.com/EnzoTironi/zoen-native/pull/58), `5f380a2`, draft on PR 56 | The exact local PostgreSQL/FDB/HTTP run passed all ten C4 output and thirty financial scenarios. [Current Linux CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38154455239) failed: 421 tests passed, one failed; the short native grant expired before the prescribed output acknowledgement boundary. The known-commit probe remained zero. Current-main composition and fresh complete Linux acceptance remain required. Public native output, managed custody and restoration continuity remain closed. |
| Historical backend proposals | PRs 46, 48 and 51–56 | Their registration, approval, financial and certified-runtime changes are included in the later review stack or await reconciliation. Original per-head results remain in the [06:10 snapshot](history/roadmap-2026-10-11-0610.md). A later local pass does not override a failed earlier workflow or establish production activation. |
| Documentation reconciliation | [PR 59](https://github.com/EnzoTironi/zoen-native/pull/59), merged before PR 60; this update starts from `fa4eed8` | The README, roadmap and Live Page contract distinguish merged core, parallel clients, local experiments and production gates. Prior snapshots remain dated history. The HIG source auditor is advisory. |
| Temporary integration | [PR 42](https://github.com/EnzoTironi/zoen-native/pull/42), `d9ebb3e` | Historical combined validation only. Reconcile after original feature PRs integrate. It is not the current backend baseline. |

Before integrating a workstream, compare its published head, production source and generated bindings with the recorded checks, then inspect the resulting main tree. Older green runs do not certify a later edit.

## Current local candidates

| Candidate | Evidence | Required next proof |
| --- | --- | --- |
| Native Page editor over PR 44 and merged PR 60 | Whole Mac Debug and ARM64 iOS Simulator Debug apps compile with matching current-core bindings. Eight Unicode/display and ten encrypted checkpoint/recovery journeys pass. The unsigned Mac vault reloads an existing development key; Release file fallback remains disabled. Actual Mac UI reproduced a stale history-sheet permission capture, and source now presents the item and permission together. | Observe corrected restore/history/reopen in the current executable. Test iOS runtime, Release Keychain/file protection, IME/undo, accessibility, minimum OS and independent-device transport. Compilation and companion tests do not establish those workflows. |
| Canonical Plan Live Pages | PR 60 integrates the shared document/session foundation and typed Plan model. Public Plan Item endpoints still refuse use. The card pattern in PR 44 opens the existing Plan editor. | Migrate every production reader/writer, typed client DTO and approval reference, preserve exact cents/IDs and mixed-version refusal, then run complete native/browser/two-device journeys. |
| Backend output-expiry fixture redesign | A source-only proposal separates preparation from the actual short signed output window after Linux failed to reach that boundary. It explicitly changes the fixture's initial grant lifetime from 1.7 to 20 seconds; the same grant must have at most 1.7 seconds remaining before fresh output admission. Production caps, leases and strict refusal assertions stay unchanged. | Fresh exact-source local and Linux results. This proposal is not the prior short-initial-grant proof, a passed test or an integrated source. |
| Native HIG work | Earlier source-qualified Mac journeys cover inspector dismissal, keyboard actions, themes, contrast and Back placement. The authorized temporary Keyboard Navigation preference was restored. | Current-main runtime journeys, VoiceOver order, enlarged text, IME/undo, accessibility preferences and Release/minimum-OS acceptance. The advisory scanner and focused Mac proof do not certify 100% HIG compliance. |

HIG release acceptance is tracked in the [platform matrix](product/hig-compliance.md). The [locked hig-doctor auditor](../tools/hig-audit/README.md) supplements current Apple guidance and native runtime evidence.

## Implemented foundations and unfinished gates

| Area | Implemented | Completion gate |
| --- | --- | --- |
| M1 accounts and sync | Device identities, signed binary events, durable outbox, paged sync and real relay/client journeys. | Release onboarding, interrupted sessions, account lifecycle and supported upgrades on each platform. |
| M2 encryption and recovery | OpenMLS direct/group encryption, pruning, explicit enrollment/revocation, paginated encrypted history, encrypted backups and peerless MLS recovery. Current merged-core M2 and claim suites pass. | Native restore/link/revoke UI and subsequent-message proof, production password-vault authority, enrollment/recovery failures and release journeys. |
| S1–S9 relay | FoundationDB ordering/dedupe, persisted renewable ownership, fencing, bounded routing, NATS fan-out, limits and telemetry. | Mixed TLS workloads, reconnect storms, authorization/lease capacity, placement, cell migration and regional failure recovery. |
| M3 agents | Grant evaluation, budgets, signed tools, model adapter and sandbox providers; financial/certified-runtime stack in review. | Managed authenticated custody, restore continuity, productive driver lifecycle, streaming/tools, accepted-chain approval reconciliation and live provider/usage reconciliation. |
| M4 media | Encrypted content-addressed blob transport, profile/background and file support; Android native media workflows under review. | Apple photo/voice delivery, interruption/resume and real object-store interoperability. |
| M5 push | Delivery design and infrastructure seams. | Token lifecycle, APNs/FCM workers, encrypted notifications, retry/collapse behavior and physical-device proof. |
| Pages, files and mini-apps | Merged shared Binary Page session, Loro editing, versions, file viewers, declarative views and sandboxed local mini-apps. | Complete native editor, canonical Plan Live Pages, Notion-style block UX, nested pages/sharing, live bindings, guarded agent edits, two-device transport and remote MCP/catalog installation. See the [Live Page contract](product/live-pages.md). |
| Platforms | Native iOS/macOS; Android and themed web shell under review. | Authenticated browser client, encrypted browser storage/enrollment, current ABI builds, native/web interoperability and distribution. |
| Communities and commerce | Conversation and permission foundations; unified Chats UI under review. | Discovery, moderation/reporting, shared-resource ACLs, business integrations, payment/credit ledger and provider reconciliation. |
| Growth | Invites, source attribution, experiments and content-free metrics. | Complete client acquisition flows, eligibility/abuse controls, bounded cell analytics and measured retention. |
| M7 operations | Infrastructure, migrations, staging, health endpoints and telemetry. | Compatible rollout, whole-stack deployment, alerts, admission, restore drills, regional recovery and cost reconciliation. |

## Verification boundaries

A passing core suite, a compiler result and an actual app journey establish different things. PR 60's Linux tree matches merged main; the native editor compiler and companion tests use a separate candidate. PRs 38, 44 and 41 have their own published versions. Backend PR 58's local pass remains separate from its Linux failure and its untested composition with current main.

The actual native ten-journey UI run used an isolated demonstration account: five reading cases, community navigation, Cards/List/Cards, fixed pinned-widget geometry and mini-app opening/closing, plus chat/Home editing and pin persistence. It retains its original executable scope. Browser screenshots currently show sample data. Neither establishes authenticated web messaging or current recovery cryptography.

The [06:10 snapshot](history/roadmap-2026-10-11-0610.md) preserves earlier refill failures, diagnostic runs, local editor/checkpoint journeys and per-head backend results. The [9 October audit](history/roadmap-2026-10-09.md) records earlier source review. Current ledger entries supersede their completion states while preserving their evidence.

## Production deployment

The public staging relay was last observed advertising protocol 2, while current shared clients require protocol 4. An `ok` health check does not establish compatibility. A reviewed rollout must inventory existing clients, apply compatible migrations, verify protocol negotiation and run fresh encrypted account/device journeys. An image rollback alone does not reverse migrations or newly written history.

Backup recovery-key mechanics and peerless MLS recovery are implemented. Password recovery is off by default and requires an explicit development opt-in. Production activation remains blocked by independent, identity-bound recovery authorization and an OPRF authority that enforces counters across nodes and proves real key destruction, as described in [ADR 0046](adr/0046-encrypted-backup.md). Deleting a staging database key does not erase historical copies.

Revocation stops fresh requests and future deliveries. Work admitted before its commit can finish because PostgreSQL device authorization and FoundationDB log append are separate transaction domains. Recovered identities and independently enrolled devices need their own authority lifecycle. See [ADR 0045](adr/0045-linking-devices.md) and [ADR 0047](adr/0047-mls-peerless-recovery.md).

Zoen is the product and built-in agent name. Legacy `roda-*`, `RodaCore`, build-task and storage/protocol identifiers still exist for technical compatibility. The [naming migration](dev/naming.md) is unfinished; branding must preserve old account data, generated bindings and authenticated history.

## Capacity evidence

The [load model](plan-real.md#scale-built-for-a-billion-people) assumes 500 million daily users, 700,000 peak messages/second, 3.5 million device deliveries/second and 160 million concurrent sockets. These are design inputs.

[ADR 0022](adr/0022-capacity.md) and [ADR 0023](adr/0023-per-space-sequencer.md) measured short development-box runs. The S9 sweep delivered 4,000 messages/second at p99 324 ms, above the 200 ms in-region target. One hot conversation reached about 3,180 messages/second. The 37 KiB/socket figure excludes TLS. The 150,000 sockets/node assumption and approximately $470,000/month projection do not establish production capacity or a bill; the cost projection excludes AI and media.

Persisted ownership now replaces process-local claims. Its bounded queues and renewable leases improve the implementation, but do not prove fleet renewal throughput, failover time, region loss recovery or the load model above.

## Required completion sequence

1. Integrate reviewed native reading/shell work with current main. Verify current native editor, recovery/linking and Android release/device journeys.
2. Finish canonical Plan Live Pages, caller migration, authenticated browser editing and shared client interoperability.
3. Close the backend output-expiry validation, compose the reviewed runtime stack with current main, then implement managed custody/continuity and the productive model/tool/approval lifecycle.
4. Complete encrypted photo/voice transport and push between independent devices.
5. Complete files, remote MCP/catalog, communities and commerce with permissions and cross-platform journeys.
6. Prove a production cell under mixed TLS workloads, hot conversations, reconnects and service failures; distinguish offered, accepted and delivered work.
7. Prove cell placement/migration, compatible rolling upgrades, disaster recovery and regional failure behavior.
8. Validate sustained capacity, headroom and full costs against the billion-user model and operating SLOs.

External signing/APNs/provider/sandbox credentials activate their live checks. Independent local, protocol, simulator and failure testing can continue. Track each gate by its evidence instead of a completion percentage.
