# Roadmap status: the complete Zoen product

Reviewed on 12 October 2026 after PRs 67, 68 and 69 merged. The target is one billion monthly users. Completion requires working client flows, release/accessibility journeys and production evidence.

## Current versions

| Workstream | Current revision | Accepted evidence and remaining boundary |
| --- | --- | --- |
| Main backend and shared core | [`a90bb52`](https://github.com/EnzoTironi/zoen-native/commit/a90bb52a1b588a91d32e4225f2a29cadc04f36b5), PR 69 merged; tree `9fab3572` | [Complete PR 69 CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38302482067): 69 standard summaries, 573 passed, zero failed/filtered and one original cross-process helper ignored. All 198 named FFI cases match accepted Main 68. Custom log-store 10 and ownership 17 remain separate. Tested, published and merged whole trees match. Native 69 retains its separately reviewed Core193 build/runtime scope. |
| Backend/Page composition | [PR 63](https://github.com/EnzoTironi/zoen-native/pull/63), `9709b7f`, merged as `0b349d3` | [Complete CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38273472655):558 standard passes and all183 FFI cases. Incorporates the reviewed backend stack, Page core and admission corrections. PRs46,48,51–56 and58 are closed as superseded, with original heads/bodies/branches and visual evidence retained. Managed custody/continuity and productive runtime activation remain open. |
| Integrated Apple/web interface | [PR 67](https://github.com/EnzoTironi/zoen-native/pull/67), `15a58f0`, merged as `a53b8ce`; native product source matches `eec5c432` | [Complete CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38294124686): 568 standard passes and all 193 FFI cases. Its tested tree equals the resulting merge tree. Core193 bindings, three full arm64 Debug builds and eleven Swift/API cases retain their scope. Actual Mac Plan, Escape, themes, notifications and nine TextKit/editor assertions pass. |
| Integrated native keyboard controls | [PR 69](https://github.com/EnzoTironi/zoen-native/pull/69), `c9cf97e`, merged as `a90bb52`; product source matches `19836654` | Three full arm64 Debug builds with Core193 and three actual Mac Tab/Space journeys pass: request review/back with focus return, change-event opening and quoted-message navigation. All 426 product and nine validation-source inputs retain their accepted scope; temporary Keyboard Navigation was restored. |
| Current native motion follow-up | Source `47c90fb`; composed with Main 69 as `883713a` | Three full arm64 Mac, iOS simulator and generic iOS device Debug builds pass with Core193. Three bounded actual Mac journeys verify the chat menu/quote jump, thread reply/Escape/cold reopen and Cards/List/Cards with filters and zero decisions. All 426 product and nine validation-source files are literal after composition. Root changed no system preferences; actual Reduce Motion/Transparency/Contrast and current-core native packaging remain open. |
| Scoped canonical approval follow-up | Frozen source `42b689b`; tree `be05f5c3`; separate backend worktree | Complete original-policy local FFI run: 216 passed, zero failed/ignored/filtered. Independent review retains all 198 prior actual names and literal bodies, verifies all 18 added cases and all 17 migrated caller sites, including both Rust native authority loops. Confirmed decisions use their own authenticated conversation prefix; ambiguous current lookups stay closed. Published-head CI/composition and actual native packaging remain required. |
| Earlier native reading/shell PRs | [PR 38](https://github.com/EnzoTironi/zoen-native/pull/38), `7a22527`; [PR 44](https://github.com/EnzoTironi/zoen-native/pull/44), `2963b246` | Both exact heads are verified ancestors of current Main and are superseded by merged PR 67. Their branches/history/media and prior Linux/native/browser results retain original scope. Browser preview uses sample data. |
| Android | [PR 41](https://github.com/EnzoTironi/zoen-native/pull/41), `e2a5406`, draft | Android 28/35 and Rust CI pass. Earlier physical S23+ evidence belongs to `302b2f0`. Current-main integration, current ABI/release/device proof and public relay compatibility remain open. |
| Separate proposals | [PR 57](https://github.com/EnzoTironi/zoen-native/pull/57), `b6fe0eb`; [PR 42](https://github.com/EnzoTironi/zoen-native/pull/42), `d9ebb3e` | Metrics UTC correction and historical integration retain independent review boundaries. They do not establish the current product baseline. |

Before merging, verify the immutable head, complete raw result, production source/generated bindings and prospective main tree. An earlier green run cannot certify changed source.

## Current native and Page acceptance

The replacement Plan screen reads typed document blocks, preserves exact text/UTF-16 marks and signed integer cents, and uses the same card/host as other items. Core193 distinguishes authenticated document sessions from this account's durable pending legacy preview. The legacy admission flag mirrors existing writer checks; it preserves successive offline edits and own pending creation without manufacturing a shared manifest or opening canonical writes.

Actual Mac Debug journeys on macOS 27.2 saved a one-cent row, quit/reopened the app, reopened its editor, canceled an unsaved edit, removed the row through confirmation and restored its prior version as a new version. All13 versions were observed across the history viewport. History Escape retains the Plan; a subsequent Escape closes the inspector. The create menu and line editor retain their own cancellation. Light/dark/light, conversation opening and Cards/List/Cards were checked. [Current images/video](https://github.com/EnzoTironi/zoen-native/pull/66#issuecomment-6130391154) use the owned demo database and explicitly identify the video as a screenshot walkthrough.

The same accepted product source also passed actual TextKit typing/undo, Unicode paste, save/cold reopen, read-only version preview and restoration as a new seventh version. Current follow-up journeys use native buttons: Tab/Space opens a request review with focus on Back; Back returns focus to its original list button. The change-event button opens the current Plan inspector. A quoted-message button receives Tab focus and jumps to the correct highlighted message. Reply creation in that journey still uses the existing swipe gesture. The temporary Keyboard Navigation preference was restored to its original value; no approval was executed.

The motion follow-up's own captures verify the chat menu, quoted-message jump and a thread reply sent with Cmd-Return, reopened through its native chip and retained after an observed process exit/cold reopen. Thread initiation still uses a swipe. Notifications start as cards and switch through list/task/approval filters back to cards; all three requests remain undecided. The video is a 28-second walkthrough of eight unchanged actual screenshots and a separate provenance title. These captures do not establish behavior with Reduce Motion enabled.

These journeys do not complete physical iOS, Release Keychain/file protection, minimum-OS, VoiceOver, IME, the full selection/viewport or keyboard/settings matrix, independent-device or full HIG acceptance. The [HIG matrix](product/hig-compliance.md) keeps those cases open. Earlier native/Core60 and Core134 footage and compiler results remain historical.

PR 68 integrates the private authenticated `confirmed::Prefix` fold and five actually executed cases for bounded history, ordered membership, stable version addresses, payload frontiers and invalid prefixes. The separately reviewed backend unit migrates all 17 linked-approval callers and both Rust native ordered loops to typed decision outcomes and an atomic unresolved-decision barrier. Its complete local 216-case run passes, including late valid/role-refused cross-conversation collisions, opposing scoped request decisions, file reopen and native image restoration/media repair. That unit is awaiting published-head CI/integration. Canonical Plan mutation APIs, history/restore and actual Swift scoped-request presentation remain open. The [Page contract](product/live-pages.md) and [session API](dev/live-page-session.md) define the remaining behavior.

## Product completion gates

| Area | Current foundation | Still required |
| --- | --- | --- |
| M1 messaging | Signed events, encrypted accounts, durable outbox, relay sync and causal claims. | Production rollout, release journeys, abusive/offline/reconnect workloads and operating SLOs. |
| M2 devices and recovery | Enrollment/revocation, paginated encrypted history and peerless recovery; current claim/M2 suites retain their original workloads. | Native recovery/device controls, independent-device flows and production recovery authority. |
| M3 agents | Grants, model gateway, sandbox tiers, SQL budgets and certified-runtime source. | Managed custody, nonrollback restore continuity, durable productive model/tool/approval driver, streaming/children and provider reconciliation. |
| M4 media | Encrypted content-addressed transport and file/profile foundations. | Apple photo/voice, interruption/resume and real object-store interoperability. |
| M5 push | Delivery design and infrastructure seams. | APNs/FCM token lifecycle, encrypted notifications, retries/collapse and physical-device proof. |
| Pages and mini-apps | Shared document session, observed retries, encrypted drafts, versions, typed Plan consumers and common cards. | Canonical Plan writers/approval replay, complete block UX/comments/nested pages/sharing/live bindings, remote MCP/catalog and two-device convergence. |
| Platforms | Native Apple clients; Android and themed web shell under review. | Authenticated browser/enrollment/storage, current Android bindings/device journeys, native/web interoperability and distribution. |
| Communities and commerce | Conversation/permission foundations and unified Chats integrated through PR 67. | Discovery, moderation/reporting, shared-resource ACLs, business workflows, payment/credit ledger and live reconciliation. |
| Growth | Invites, source attribution, experiments and content-free metrics. | Complete acquisition/eligibility/abuse flows, bounded analytics and measured retention. |
| M7 operations | Infrastructure, migrations, staging, health and telemetry. | Compatible deployment, alerts/admission, restore drills, region recovery and full costs. |

## Deployment and capacity

The last recorded public staging snapshot advertised protocol 2; current clients require protocol 4. That historical health observation is not current compatibility evidence. A rollout must inventory clients, apply compatible migrations, verify negotiation and run fresh encrypted account/device flows. Image rollback alone does not reverse migrations or signed history.

Backup recovery-key and peerless MLS mechanics are implemented. Password recovery is off by default and requires an explicit development opt-in. Production needs independent identity-bound recovery authorization and an OPRF authority enforcing counters across nodes and proving key destruction; see [ADR 0046](adr/0046-encrypted-backup.md). Revocation and admitted work cross Postgres/FDB transaction boundaries; [ADR 0045](adr/0045-linking-devices.md) and [ADR 0047](adr/0047-mls-peerless-recovery.md) retain their lifecycle rules.

The [load model](plan-real.md#scale-built-for-a-billion-people) assumes 500 million daily users, 700,000 peak messages/second, 3.5 million deliveries/second and 160 million sockets. These are design inputs. [ADR 0022](adr/0022-capacity.md) and [ADR 0023](adr/0023-per-space-sequencer.md) record short development-box runs: 4,000 messages/second at p99 324 ms and roughly 3,180 messages/second for one hot conversation. The 37 KiB/socket result excludes TLS. The 150,000 sockets/node assumption and approximately $470,000/month projection exclude AI/media and do not establish a production bill or capacity.

Renewable ownership and bounded queues improve the implementation; fleet renewal, sustained TLS throughput, failover, region loss, headroom and total costs require representative deployment drills.

## Completion sequence

1. Integrate the reviewed motion/backend follow-ups; package current core libraries and complete recovery/linking, Android release/device and full HIG journeys.
2. Finish canonical Plan/approval migration, authenticated web and shared-client interoperability.
3. Implement managed custody/continuity and the durable productive agent lifecycle over the integrated backend; reconcile live provider usage.
4. Complete independent-device encrypted photo/voice and push.
5. Complete shared files, remote MCP/catalog, communities and commerce with permissions and cross-platform flows.
6. Prove production cells under mixed TLS load, hot chats, reconnects and service failures; separate offered, accepted and delivered work.
7. Prove placement/migration, rolling upgrades, restore and regional failure behavior.
8. Validate sustained capacity, headroom and complete costs against the billion-user model and SLOs.

External signing/APNs/provider credentials activate live checks; independent source, local failure and client work continues. Track evidence per gate instead of a completion percentage. [Previous documentation](history/roadmap-2026-10-12-before-consolidation.md) and [dated audits](history/roadmap-2026-10-11-0610.md) retain earlier failures and per-head results. [Naming migration](dev/naming.md) preserves old account/protocol identifiers while user-facing branding remains Zoen.
