# Roadmap status: the complete Zoen product

Current execution, 26 October 2026: **finish iOS first**, using React Native/Expo,
the shared Rust core and the real backend. The active sequence and release gates
are maintained in the [complete-product goal](dev/zoen-complete-product-goal.md#execução-atual--20-de-outubro-de-2026)
and [acceptance manifest](../apps/zoen/acceptance.json). Web/Android follow the iOS
release gate; desktop follows later. Telegram iOS guides UI and interactions;
Signal guides reliability; Notion/ChatGPT Space guide collaborative creation.
Enzo approved consolidating existing work before new capabilities: membership
and revocation, offline creation, then native Page creation/recovery. Anchored
comments wait for those dependencies. Internal ArtCraft work stays stopped;
editors are external independently released MCP Apps.

The installed Simulator artifact is701, with partial acceptance; membership703
is under qualification and is not installed. The maintained
[execution plan](../apps/zoen/acceptance/parts/scope/execution-plan.json) and
[current iPhone artifact](../apps/zoen/acceptance/parts/artifacts/current-iphone-artifact.json)
separate local source/build evidence, native observations and pending acceptance.
Physical iPhone and public-backend release remain open. GitHub publication awaits
connectivity and fresh remote-state checks; local work is not a completed merge.

## Historical checkpoints

The older checkpoint sections
below retain their original dates, artifacts and limits; their native-client,
PR and billion-user descriptions do not define the current execution plan.

Reviewed on 12 October 2026 after PRs 67, 68, 69 and 71 merged. The target is one billion monthly users. Completion requires working client flows, release/accessibility journeys and production evidence. New implementation uses one branch per unit. The coordinator merges that branch into `main` after the unit's proof, following the pstack playbooks.

## Shared-main handoff checkpoint

The user requested publishing all current coordinated source and then pausing. The [complete handoff](dev/handoff-2026-10-12.md) records exact fronts, checks, open defects, local packets and resume commands. Fresh tests pass 13 web behaviors, 17 portable MLS, three web-Core Rust, seven attachment, five native push-boundary and six Android-verifier cases. Swift grammar passes for 17 current files and Rust formatting passes. These results do not complete current Core/CLI, bindings, browser pairing, Android device, native cleanup UI, provider delivery or HIG acceptance. A final Mac build attempt stopped at its unchanged disk preflight.

The current Core full231 run was interrupted by a source guard; Store9, Proto28 and focused bounds9 passed. The hosted PR70 file-import failure remains open. The seven local attachment cases now cover explicit discard despite retained preview leases, while the earlier actual cancel-cleanup failure still needs a new UI run. Inline-reply Escape passed on v21. Native render caching and typed-background-only cleanup have source review and grammar checks, with no fresh performance claim.

Zoen has no production users/data. The [prelaunch policy](dev/prelaunch-policy.md) replaces compatibility-shim/backfill plans: update internal interfaces and all callers/tests atomically; coordinate any migration-baseline reset explicitly. The retained verified baseline below describes its original source, rather than certifying this later checkpoint.

## Current versions

| Workstream | Current revision | Accepted evidence and remaining boundary |
| --- | --- | --- |
| Verified product baseline | [`ff5b519`](https://github.com/EnzoTironi/zoen-native/commit/ff5b519f1734e9d28018ab33d3919d12c9147e45), PR 71 merged; tree `12924649` | [Complete PR 71 CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38309704131): 69 standard summaries, 573 passed, zero failed/filtered and one original cross-process helper ignored. All 198 named FFI cases retain the accepted Main 69 scope. Custom log-store 10 and ownership 17 remain separate. Tested, published and merged whole trees match. |
| Backend/Page composition | [PR 63](https://github.com/EnzoTironi/zoen-native/pull/63), `9709b7f`, merged as `0b349d3` | [Complete CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38273472655):558 standard passes and all183 FFI cases. Incorporates the reviewed backend stack, Page core and admission corrections. PRs46,48,51–56 and58 are closed as superseded, with original heads/bodies/branches and visual evidence retained. Managed custody/continuity and productive runtime activation remain open. |
| Integrated Apple/web interface | [PR 67](https://github.com/EnzoTironi/zoen-native/pull/67), `15a58f0`, merged as `a53b8ce`; native product source matches `eec5c432` | [Complete CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38294124686): 568 standard passes and all 193 FFI cases. Its tested tree equals the resulting merge tree. Core193 bindings, three full arm64 Debug builds and eleven Swift/API cases retain their scope. Actual Mac Plan, Escape, themes, notifications and nine TextKit/editor assertions pass. |
| Integrated native keyboard controls | [PR 69](https://github.com/EnzoTironi/zoen-native/pull/69), `c9cf97e`, merged as `a90bb52`; product source matches `19836654` | Three full arm64 Debug builds with Core193 and three actual Mac Tab/Space journeys pass: request review/back with focus return, change-event opening and quoted-message navigation. All 426 product and nine validation-source inputs retain their accepted scope; temporary Keyboard Navigation was restored. |
| Integrated native motion | [PR 71](https://github.com/EnzoTironi/zoen-native/pull/71), `4754678`, merged as `ff5b519` | Three full arm64 Mac, iOS simulator and generic iOS device Debug builds pass with Core193. Bounded actual Mac journeys verify chat menu/quote jump, thread reply/Escape/cold reopen and Cards/List/Cards with filters and zero decisions. [Actual settings checks](https://github.com/EnzoTironi/zoen-native/pull/71#issuecomment-6132977105) cover Reduce Motion, Reduce Transparency and Increase Contrast; all temporary values were restored. |
| Scoped canonical approval follow-up | Frozen source `42b689b`; [PR 70](https://github.com/EnzoTironi/zoen-native/pull/70), published `ed607a5`; now composed in shared main's pending work | Complete local FFI run: 216 passed, zero failed/ignored/filtered, with independent source review of all 18 additions and 17 migrated callers. [Hosted run](https://github.com/EnzoTironi/zoen-native/actions/runs/38309689759) has 590 standard passes, one file-import failure and one original ignored helper; all 216 FFI cases pass. The missing imported `plan.md` failure remains open. A separate current-main local files journey passed with 86 exact documents; it does not resolve the hosted failure. |
| Earlier native/Core216 units | Native source `b6faf26`, followed by v20/v21 source units | Reviewed platform-26 Core216 package retains the previous Swift ABI. Three full arm64 Debug builds pass at the earlier units. Actual native-menu Tab/Space and bounded VoiceOver open the correct reply/thread; temporary settings were restored. Inline-reply Escape failed on v20 and passed on v21. The latest attachment lease/render changes have Swift grammar and local attachment checks, but no fresh app build/runtime acceptance. |
| Earlier native reading/shell PRs | [PR 38](https://github.com/EnzoTironi/zoen-native/pull/38), `7a22527`; [PR 44](https://github.com/EnzoTironi/zoen-native/pull/44), `2963b246` | Both exact heads are verified ancestors of current Main and are superseded by merged PR 67. Their branches/history/media and prior Linux/native/browser results retain original scope. Browser preview uses sample data. |
| Android | [PR 41](https://github.com/EnzoTironi/zoen-native/pull/41), `e2a5406`, preserved; source imported on shared main | Earlier Android 28/35 and Rust CI retain their scope. Current Plan projection/save/history and matched-source ABI verifier are ready for integration. Six verifier cases pass and old JNI is rejected for missing current exports. Current Kotlin builds, emulator/device journeys, release and public relay compatibility remain open. |
| Authenticated web | Shared-main enrollment, vault and portable MLS work | Fresh typechecking and 13 TS behavior tests pass; 17 portable MLS and three web-Core Rust cases also pass within native-library scope. Independent review found reconnect/generation/key-replacement issues; source corrections are applied. Actual native/browser pairing, MLS interoperability, reload/disconnect journeys and shared Page UX remain open. |
| Apple attachments | Shared-main import/review/send source | Seven real Swift filesystem/ImageIO journeys pass: private staging and preview lifetime, bounded copies/images, cancellation, FIFO/symlink refusal and abandoned-session cleanup preserving live sessions. Earlier v21 Debug builds passed before the final discard/lease change. Actual Cancel left a staged copy; the correction requires a new app build/UI journey. Current bounded Core reception, camera/Photos permission flows, voice and independent-device delivery remain open. |
| Separate proposals | [PR 57](https://github.com/EnzoTironi/zoen-native/pull/57), `b6fe0eb`; [PR 42](https://github.com/EnzoTironi/zoen-native/pull/42), `d9ebb3e` | Metrics UTC correction and historical integration retain independent review boundaries. They do not establish the current product baseline. |

Before publishing each direct-main unit, freeze its owned paths, inspect the exact change, verify relevant behavior and current generated bindings, and preserve other agents' pending work. Existing PRs keep their original evidence. An earlier green run cannot certify changed source.

## Current native and Page acceptance

The replacement Plan screen reads typed document blocks, preserves exact text/UTF-16 marks and signed integer cents, and uses the same card/host as other items. Core193 distinguishes authenticated document sessions from this account's durable pending legacy preview. The legacy admission flag mirrors existing writer checks; it preserves successive offline edits and own pending creation without manufacturing a shared manifest or opening canonical writes.

Actual Mac Debug journeys on macOS 27.2 saved a one-cent row, quit/reopened the app, reopened its editor, canceled an unsaved edit, removed the row through confirmation and restored its prior version as a new version. All13 versions were observed across the history viewport. History Escape retains the Plan; a subsequent Escape closes the inspector. The create menu and line editor retain their own cancellation. Light/dark/light, conversation opening and Cards/List/Cards were checked. [Current images/video](https://github.com/EnzoTironi/zoen-native/pull/66#issuecomment-6130391154) use the owned demo database and explicitly identify the video as a screenshot walkthrough.

The same accepted product source also passed actual TextKit typing/undo, Unicode paste, save/cold reopen, read-only version preview and restoration as a new seventh version. Current follow-up journeys use native buttons: Tab/Space opens a request review with focus on Back; Back returns focus to its original list button. The change-event button opens the current Plan inspector. A quoted-message button receives Tab focus and jumps to the correct highlighted message. Reply creation in that journey still uses the existing swipe gesture. The temporary Keyboard Navigation preference was restored to its original value; no approval was executed.

PR 71's captures verify the chat menu, quoted-message jump and a thread reply sent with Cmd-Return, reopened through its native chip and retained after an observed process exit/cold reopen. Notifications start as cards and switch through list/task/approval filters back to cards; all three requests remain undecided. The original video is a 28-second screenshot walkthrough. A separate actual settings unit covers reduced motion/transparency and increased contrast, with its own captures and restored preferences. The current Core216 unit adds native reply/thread menu alternatives and a bounded VoiceOver action; it also preserves the observed inline-reply Escape failure until its correction is verified.

These journeys do not complete physical iOS, Release Keychain/file protection, minimum-OS, VoiceOver, IME, the full selection/viewport or keyboard/settings matrix, independent-device or full HIG acceptance. The [HIG matrix](product/hig-compliance.md) keeps those cases open. Earlier native/Core60 and Core134 footage and compiler results remain historical.

PR 68 integrates the private authenticated `confirmed::Prefix` fold and five executed cases for bounded history, ordered membership, stable version addresses, payload frontiers and invalid prefixes. The backend follow-up migrates all 17 linked-approval callers and both Rust native ordered loops to typed outcomes and an atomic unresolved-decision barrier. Its local and hosted 216-case FFI runs pass, including cross-conversation collisions, scoped decisions, file reopen and image restoration. The full hosted unit still fails a separate file-import journey; its correction and fresh current-main verification are required. Canonical Plan writers/history and the dedicated native duplicate-ID request journey remain open. The [Page contract](product/live-pages.md) and [session API](dev/live-page-session.md) define the remaining behavior.

## Product completion gates

| Area | Current foundation | Still required |
| --- | --- | --- |
| M1 messaging | Signed events, encrypted accounts, durable outbox, relay sync and causal claims. | Production rollout, release journeys, abusive/offline/reconnect workloads and operating SLOs. |
| M2 devices and recovery | Enrollment/revocation, paginated encrypted history and peerless recovery; current claim/M2 suites retain their original workloads. | Native recovery/device controls, independent-device flows and production recovery authority. |
| M3 agents | Grants, model gateway, sandbox tiers, SQL budgets and certified-runtime source. | Managed custody, nonrollback restore continuity, durable productive model/tool/approval driver, streaming/children and provider reconciliation. |
| M4 media | Encrypted content-addressed transport and file/profile foundations; private Apple attachment review passes local filesystem journeys and Debug builds. | Bounded reception, native send/reopen, photo/voice, interruption/resume and real object-store interoperability. |
| M5 push | Delivery design and infrastructure seams. | APNs/FCM token lifecycle, encrypted notifications, retries/collapse and physical-device proof. |
| Pages and mini-apps | Shared document session, observed retries, encrypted drafts, versions, typed Plan consumers and common cards. | Canonical Plan writers/approval replay, complete block UX/comments/nested pages/sharing/live bindings, remote MCP/catalog and two-device convergence. |
| Platforms | Native Apple clients; shared-main Android Plan and authenticated browser enrollment source are under verification. | Current Android bindings/device journeys, real encrypted browser sessions, native/web interoperability and distribution. |
| Communities and commerce | Conversation/permission foundations and unified Chats integrated through PR 67. | Discovery, moderation/reporting, shared-resource ACLs, business workflows, payment/credit ledger and live reconciliation. |
| Growth | Invites, source attribution, experiments and content-free metrics. | Complete acquisition/eligibility/abuse flows, bounded analytics and measured retention. |
| M7 operations | Infrastructure, migrations, staging, health and telemetry. | Compatible deployment, alerts/admission, restore drills, region recovery and full costs. |

## Deployment and capacity

The last recorded public staging snapshot advertised protocol 2; current clients require protocol 4. That historical health observation is not current compatibility evidence. A rollout must inventory clients, apply compatible migrations, verify negotiation and run fresh encrypted account/device flows. Image rollback alone does not reverse migrations or signed history.

Backup recovery-key and peerless MLS mechanics are implemented. Password recovery is off by default and requires an explicit development opt-in. Production needs independent identity-bound recovery authorization and an OPRF authority enforcing counters across nodes and proving key destruction; see [ADR 0046](adr/0046-encrypted-backup.md). Revocation and admitted work cross Postgres/FDB transaction boundaries; [ADR 0045](adr/0045-linking-devices.md) and [ADR 0047](adr/0047-mls-peerless-recovery.md) retain their lifecycle rules.

The [load model](plan-real.md#scale-built-for-a-billion-people) assumes 500 million daily users, 700,000 peak messages/second, 3.5 million deliveries/second and 160 million sockets. These are design inputs. [ADR 0022](adr/0022-capacity.md) and [ADR 0023](adr/0023-per-space-sequencer.md) record short development-box runs: 4,000 messages/second at p99 324 ms and roughly 3,180 messages/second for one hot conversation. The 37 KiB/socket result excludes TLS. The 150,000 sockets/node assumption and approximately $470,000/month projection exclude AI/media and do not establish a production bill or capacity.

Renewable ownership and bounded queues improve the implementation; fleet renewal, sustained TLS throughput, failover, region loss, headroom and total costs require representative deployment drills.

## Completion sequence

1. Publish verified shared-main units; resolve the file-import failure, regenerate current core bindings and complete recovery/linking, Android release/device and full HIG journeys.
2. Finish canonical Plan/approval migration, authenticated web and shared-client interoperability.
3. Implement managed custody/continuity and the durable productive agent lifecycle over the integrated backend; reconcile live provider usage.
4. Complete independent-device encrypted photo/voice and push.
5. Complete shared files, remote MCP/catalog, communities and commerce with permissions and cross-platform flows.
6. Prove production cells under mixed TLS load, hot chats, reconnects and service failures; separate offered, accepted and delivered work.
7. Prove placement/migration, rolling upgrades, restore and regional failure behavior.
8. Validate sustained capacity, headroom and complete costs against the billion-user model and SLOs.

External signing/APNs/provider credentials activate live checks; independent source, local failure and client work continues. Track evidence per gate instead of a completion percentage. [Previous documentation](history/roadmap-2026-10-12-before-consolidation.md) and [dated audits](history/roadmap-2026-10-11-0610.md) retain earlier failures and per-head results. [Naming migration](dev/naming.md) preserves old account/protocol identifiers while user-facing branding remains Zoen.
