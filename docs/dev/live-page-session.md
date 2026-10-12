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
| `LocalPendingLegacy` | This account's signed durable optimistic legacy intent, including its own pending creation. | Version 0 means creation is not confirmed. The namespaced presentation hash is not an edit context/shared manifest. Presentation readiness is not remote confirmation. |

`legacy_edit_admitted` mirrors the real existing legacy writer gates, including writer/author/account checks and valid totals. It preserves successive offline edits and own pending creation without creating a second editable store. Reader, removed-member, other-account, private/unreadable and saved/pending canonical-cutover cases remain read-only. Native delayed controls reread the observed version/hash before the legacy mutation. This flag grants no financial or provider authority.

Legacy edit/toggle/add/remove/restore/linked-approval mutations refuse before changing storage after saved or pending cutover. A poll refuses before adding a legacy row or closing/posting its result. Approval display retains exact first-matching row text/cents; completion/formatting alone do not change the financial hash.

PR 68 extracts the existing confirmed fold into private `confirmed::Prefix`, validating the conversation and available `[0, end)` event range before reconstructing membership, versions and accepted content. The existing timeline adapter passes its full confirmed log and then applies the unchanged pending overlay. This private reader does not activate a canonical writer or approve a request. A separate backend unit is migrating all 17 approval callers and both native ordered loops to bounded decision reads and an atomic barrier at the first unresolved decision; its complete execution remains under validation. Canonical history/restore and writer migration remain closed. Financial metadata must never be flattened into ordinary paragraph text.

## Verification and remaining workflows

[PR 68's complete CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38296184197) passes 573 standard tests, zero failed/filtered and one original helper ignored. Its 198 actual FFI names equal the accepted prior 193 plus five bounded-prefix cases. Custom log-store 10/ownership 17 counts remain separate. Main `c87542f` incorporates Native 67 and differs from that hosted whole tree; all 282 Rust/Cargo/CI inputs remain byte-identical. The native consolidation has its own complete CI and client evidence.

Core193 Swift libraries/bindings and three full arm64 Mac, iOS simulator and generic iOS device Debug builds pass for native source `19836654`. The unchanged eleven Swift/API cases retain their exact Unicode/UTF-16 text, signed cents, duplicate/orphan routing and generated-read scope. Owned-demo Mac journeys at the earlier accepted product source verify one-cent save/cold reopen/re-edit, cancellation, confirmed removal and restoration as a new version, plus actual TextKit typing/undo, Unicode paste, saved cold reopen, read-only version preview and restoration. [Images/video](https://github.com/EnzoTironi/zoen-native/pull/67) retain that scope. Current change-event keyboard opening reaches the same Plan inspector; it does not open the historical version sheet.

Core198 native libraries/bindings, physical iOS, Release Keychain/file protection, IME, full selection/viewport behavior, reader/revocation UI and independent-device convergence remain open. Next workflows include distinct members editing separate/overlapping blocks, offline reconnect, approve/deny/undo of an agent suggestion, revocation, unchanged/failed refresh and nested-page moves. The [product contract](../product/live-pages.md), [HIG matrix](../product/hig-compliance.md) and [roadmap](../roadmap-status.md) retain their separate acceptance gates. [History](../history/roadmap-2026-10-12-before-consolidation.md) preserves earlier failures and results.
