# Live Page document session

Binary Pages use one session for saved history, observed edits, encrypted draft receipts, recovery and publication. Main `c87542f` includes the shared session, typed Plan compatibility and private authenticated confirmed-history prefixes. The reviewed native app still packages Core193; current Main has 198 executed FFI cases. Canonical Plan mutation remains a separate admission gate.

## Client contract

1. Read `page(item_id)` and keep its `edit_context` with the blocks actually displayed.
2. Allocate one mutation ID per edit. Call `page_apply_from` with that ID, the observed context, complete block order and changed blocks.
3. After an uncertain response, repeat the exact ID/context/edit. A retained receipt returns `replayed = true` without applying the edit twice.
4. Display the returned `page`. This editor's `applied_content_hash` and `applied_edit_context` identify its accepted operation and can differ from the merged page. Adopt a merged page's blocks and context together.
5. Publish with `page_commit`. Show `pending_sync` and `save_error`; local acknowledgement does not establish remote relay acceptance.

Contexts bind actor, conversation, item and creation. Restore fences unaccepted earlier contexts with `Stale`; an accepted exact retry can return its retained receipt while the current page remains restored. Receipt retention is 512 entries. After eviction or invalidation, reread and reconcile intent instead of assigning a fresh ID to an obsolete full document.

The older `page_apply` adapter reads current context under the Engine lock. It cannot carry what an older client observed; concurrent editors must use `page_apply_from`. Every actual revision, including own-edit undo, requires its own mutation. Matching visible text alone cannot identify an earlier edit whose response was lost.

## Persistence, recovery and media

The session retains a verified saved document and the local document. Reads authenticate the current encrypted journal before using cached state; reload changes the cache epoch. Unreadable ciphertext never becomes an empty draft. Saved content can remain readable while editing is disabled.

Publication stages blob, signed event, outbox and encrypted journal marker in one SQLite savepoint. Failure rolls back the writes and retains a recoverable draft/error. Device-derived draft journals are scoped to identity/conversation/page and excluded from portable backups; saved-version recovery does not promise unsaved-draft recovery after device loss.

PR 66 repairs a damaged media-byte cache only after the imported bytes pass their content hash. The update is atomic; wrong-hash input leaves storage unchanged. An identical reimport preserves row metadata, and ordinary media insertion retains its existing no-replacement behavior.

Existing Binary Page Loro bytes, database/preference keys and `Storage(message)` remain compatible. Regenerate bindings and libraries together. User-facing branding is Zoen; persisted technical identifiers require the separate [naming migration](naming.md).

## Typed Plan read and compatible legacy admission

`plan_page()` returns typed title/budget, summary, section, priced line and prose blocks from the same document session. Stable CRDT block IDs stay distinct from possibly duplicated legacy row IDs. Raw text, UTF-16 marks, signed integer cents, prose styles and image titles survive the boundary.

The optional total accumulates checked `i128` and converts the mathematical final result to `i64`. Compensating credits preserve valid totals regardless of order. Overflow leaves the total unavailable while preserving rows/history; the older mandatory-total `PlanDto` returns an explicit error. Historical negative cents, including `i64::MIN`, remain readable; new negative legacy additions refuse before storage changes.

Core193 exposes two read sources:

| Source | Meaning | Admission boundary |
| --- | --- | --- |
| `DocumentSession` | Authenticated saved/session content with its accepted version and private-draft state. | Canonical writes remain closed; private, waiting, unreadable or cutover sessions do not admit legacy controls. Cards/search read accepted content rather than private edits. |
| `LocalPendingLegacy` | This account's signed durable optimistic legacy intent, including its own pending creation. | Version 0 means creation is not confirmed. `content_hash` is the admitted PlanDoc binding, shared with `DocumentSession` for the same PlanDoc. A pure relay confirmation does not change it, and a private session draft does not move it. Presentation readiness is not remote confirmation. |

`legacy_edit_admitted` mirrors the real existing legacy writer gates, including writer/author/account checks and valid totals. It preserves successive offline edits and own pending creation without creating a second editable store. Reader, removed-member, other-account, private/unreadable and saved/pending canonical-cutover cases remain read-only. Legacy edit, toggle, add, and remove take that observed `content_hash` and check readiness, admission, and the hash under the same Engine lock as the write. A mismatch returns `Stale` and appends nothing. Plan restore uses `restore_plan_version` with the same check; `restore_version` refuses Plans. Undo carries the resulting binding and the item version the mutation created, and refuses when either has moved. This flag grants no financial or provider authority.

Legacy edit/toggle/add/remove/restore/linked-approval mutations refuse before changing storage after saved or pending cutover. A poll refuses before adding a legacy row or closing/posting its result. Approval display retains exact first-matching row text/cents; completion/formatting alone do not change the financial hash.

PR 68 extracts the confirmed fold into private `confirmed::Prefix`, validating the conversation and available `[0, end)` range before reconstructing membership, versions and accepted content. The timeline adapter passes its full confirmed log and then applies the pending overlay. The scoped backend unit at `42b689b` migrates all 17 approval callers and both Rust native ordered loops to bounded decision reads and an atomic barrier at the first unresolved decision. Its complete local and hosted FFI runs pass 216 cases, preserving all 198 previous names and bodies. PR 70's full hosted suite still fails a separate file-import journey; current-main correction and integration remain required. Confirmed authority uses the own-conversation prefix; ambiguous current lookups refuse while scoped history remains readable. The request ABI is `request_id`, `title`, `approved`, with no cost field. Canonical history/restore and writers remain closed. Financial metadata must never be flattened into ordinary paragraph text.

## Verification and remaining workflows

[PR 71's complete CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38309704131) passes 573 standard tests, zero failed/filtered and one original helper ignored. Its 198 actual FFI names retain the accepted Main 69 scope. Custom log-store 10/ownership 17 counts remain separate. Verified baseline Main `ff5b519` has the same whole tree as that hosted checkout and published head. The merged interface retains its Core193 client evidence; separate current Core216 packaging and three Debug builds pass within their own scope.

Core193 Swift libraries/bindings and three full arm64 Mac, simulator and generic iOS Debug builds pass for PR 69 and merged motion PR 71. The unchanged eleven Swift/API cases retain exact Unicode/UTF-16 text, signed cents, duplicate/orphan routing and generated-read scope. Owned-demo Mac journeys verify one-cent save/cold reopen/re-edit, cancellation, confirmed removal and new-version restoration, plus TextKit typing/undo, Unicode paste and read-only version preview. [Images/video](https://github.com/EnzoTironi/zoen-native/pull/67) retain that scope. [Keyboard change-event opening](https://github.com/EnzoTironi/zoen-native/pull/69) reaches the current Plan inspector. PR 71 adds chat/thread/cold-reopen, notification and actual settings journeys.

Plan writers check the observed PlanDoc hash under the same Engine lock as the write. A private draft leaves that hash in place and leaves the mutation gate open. Android passes the hash it already showed into edit, toggle, add, and remove, and restores a Plan with the hash captured when the confirmation opened. A PlanDoc change between that read and the write is `Stale`, with no new version.

Core198 native libraries/bindings, physical iOS, Release Keychain/file protection, IME, full selection/viewport behavior, reader/revocation UI and independent-device convergence remain open. Next workflows include distinct members editing separate/overlapping blocks, offline reconnect, approve/deny/undo of an agent suggestion, revocation, unchanged/failed refresh and nested-page moves. The [product contract](../product/live-pages.md), [HIG matrix](../product/hig-compliance.md) and [roadmap](../roadmap-status.md) retain their separate acceptance gates. [History](../history/roadmap-2026-10-12-before-consolidation.md) preserves earlier failures and results.
