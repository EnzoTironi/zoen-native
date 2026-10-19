# Messenger parity: implementation boundaries and evidence

Source baseline: main at `494237a`; thread-specific observations also inspect PR #97.
This is a source audit, not a claim of WhatsApp parity or a completed device journey.
No simulators or emulators were run on the laptop. Voice/media, glass styling,
revoked-device admission, iOS build/polish and relay scaling are outside this audit's changes.

## Seven independent pull requests

Each front owns one PR against main. Do not merge these PRs or force-push.
Prefer dedicated feature modules; coordinate the small integration hunks in shared
chat views, DTOs, core dispatch and localized resources before implementation.
Never hand-edit generated UniFFI bindings; regenerate from the API source.
All user-facing copy must be plain Portuguese.

| Front | Observed source / gap | Ownership / acceptance evidence |
| --- | --- | --- |
| Threads (#97, preserve existing PR) | Follow, per-thread read and inbox are implemented on its branch. Android nested replies ignore the also-in-chat flag. Unread test assertions can be skipped after a failed send. Web follow is local and its unread indicator counts historical replies, not a read watermark. | Own `client-core/threads.rs`, reply types and `MessageReplies.swift`; coordinate chat-view/core DTO hunks. Merge main into the existing branch. Assert reply success, synced read/follow on two devices, unread clearing, nested replies and also-in-chat behavior. Prior green checks do not validate a new merge. |
| Presence | Native online/typing exist; protocol lacks last-seen and privacy preference. Android keeps one typist per space. Web has no peer-presence/typing processing. | Own presence-specific protocol, relay and core modules. Coordinate `session.rs`, `api.rs`, web client and chat headers. Verify ephemeral expiry, concurrent typists, multi-device online aggregation, reconnect clearing and server-enforced last-seen visibility. Do not expose device activity timestamps as account last-seen. |
| Ticks/reactions | Native and web send receipts and fold reactions. Android quick-reaction errors are discarded. Web reaction DTO omits participants. Only six quick reactions are exposed in inspected controls. | Own receipt/reaction logic and detail/picker surfaces. Coordinate core client/DTO and message-action hunks. Test monotonic watermarks, group members, offline/reconnect, replacing/removing reactions and error feedback. Source presence is not end-to-end verification. |
| Search/edit/delete | Author-only edit/delete and search reindexing exist. Android dismisses mutation dialogs and hides errors. Apple search converts failure to empty results and does not key refresh on revision. Global message search is capped rather than conversation-scoped/paginated. Web sidebar search filters conversation titles, not message text. | Own universal/store search and dedicated search screens. Coordinate message actions with threads/reactions. Test edit → old term absent/new term present → delete → absent, replay/reopen, unauthorized operations, late edits after deletion, quoted/thread references, scoped limits and pagination. Preserve draft text on failure. |
| Gap audit (this PR) | Web pin/mute/archive preferences used one unscoped storage key, accepted invalid JSON shapes and let storage-write failures escape. Native preferences are core-backed; web preferences remain browser-local. | Fix account scoping, validate stored flags, tolerate storage denial and reset per-account component state. New deterministic tests plus web CI. Do not claim this adds cross-device preference sync. |
| Devices + link pickup | Apple/Android screens list/revoke devices but have no production connect action found. Web pairing directs users to that missing action. No unified web Devices screen found. Native pickup exits on transport errors and does not bound connection/receive waits by expiry. Web ready-state request rejection can stop polling; no pending-link expiry found. | Own Devices screens/settings navigation and pickup helpers (`net.rs`, `pairing.ts`, pending-link expiry). Coordinate settings with presence/recovery. Verify link/revoke journeys, refresh/retry failures, reconnect pickup until expiry, strict deadline and cancellation. Leave revoked-device rejection unchanged. |
| Recovery key | Core supports recovery-key generation/restoration and CLI has lost-device recovery coverage. Apple/Android activation is password-only; disabled restore copy promotes an unavailable password flow. Web lacks an account-recovery bridge (session restore is different). | Own dedicated recovery UI/onboarding modules, not device settings sections. Replace unsafe password entry points, preserve existing encrypted backups and backup-now. Test key confirmation, wrong/malformed keys, interrupted operations and real lost-device recovery. Never log/display a key outside its intended user-facing recovery flow. |

## Other WhatsApp comparison points

The inspected web UI supports quoted replies, forwards, archive/pin/mute, basic
message actions and sent/delivered/read rendering. This audit does not establish
cross-platform parity for notification controls, disappearing messages, blocking,
contact safety verification, export, drafts across devices, accessibility,
large-group behavior or all offline/reconnect cases. Treat these as unverified,
not absent. Voice/media are deliberately deferred to their existing owners.

## Shared-file coordination

- Threads: reply/thread modules; integrate its chat timeline hunks first when
  reconciling other PRs. Do not merge another feature branch into it.
- Presence: new ephemeral state and header/status hunks, not annotation folding.
- Ticks/reactions: annotation/receipt hunks, not reply or search behavior.
- Search: search modules; isolate mutation-error changes from reaction UI.
- Devices and recovery: separate new views/modules; only minimal, distinct settings
  navigation entries in shared platform files.
- Gap audit: account-local preference helper and web component boundary only;
  do not rewrite thread, reaction or receipt handlers.

Separate task checkouts are required for independent branch publication. Shared
subagents can inspect in parallel, but must not switch the lead's branch. The
laptop rejected creation of isolated task sessions while reserved by the relay
audit; none of the other fronts is considered implemented by this inventory.

## Verification for this increment

`node --test packages/zoen-ui/web/chat-preferences.test.ts` reproduced four failures
against the extracted original storage behavior, then passed all four after the fix.
`npm run typecheck:web` passed. `npm run test:web` passed all 17 tests.
The new web workflow runs these checks on pull requests. Live browser account-switch
behavior and native/platform journeys have not been exercised; no UI screenshot is
included because this increment changes state isolation, not the rendered design.
