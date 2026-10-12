# Live Page document session

Binary Pages use one session for saved history, observed edits, encrypted draft receipts, recovery and publication. Main `3c8cef9` includes the shared session and Core193 typed Plan compatibility. Canonical Plan mutation remains a separate admission gate.

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

The integrated backend contains stricter approval/runtime foundations. All17 canonical Plan approval callers, accepted-chain replay, canonical history/restore and writer migration still require reconciliation. Financial metadata must never be flattened into ordinary paragraph text.

## Verification and remaining workflows

[PR 66's complete CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38288186205) tests the same tree as Main `3c8cef9`:69 standard summaries, 568 passed,0 failed/filtered and 1 original helper ignored. All193 actual FFI names match the local full suite and retain prior 183. Ten additions cover offline admission/preview and validated cache recovery. Custom log-store 10/ownership 17 counts remain separate.

Matching Core193 Swift libraries/bindings and full arm64 Mac, iOS simulator and generic iOS device Debug builds pass. Eleven Swift/API cases exercise exact Unicode/UTF-16 text, signed cents, duplicate/orphan routing and actual generated reads/admission flags. Owned-demo Mac journeys at `eec5c432` verify one-cent save/cold reopen/re-edit, cancellation, confirmed removal and restoration as a new version; [images/video](https://github.com/EnzoTironi/zoen-native/pull/66#issuecomment-6130391154) retain that scope.

Physical iOS, Release Keychain/file protection, actual IME/undo/selection/viewport behavior, reader/revocation UI and independent-device convergence remain open. Next workflows include distinct members editing separate/overlapping blocks, offline reconnect, approve/deny/undo of an agent suggestion, revocation, unchanged/failed refresh and nested-page moves. The [product contract](../product/live-pages.md), [HIG matrix](../product/hig-compliance.md) and [roadmap](../roadmap-status.md) retain their separate acceptance gates. [History](../history/roadmap-2026-10-12-before-consolidation.md) preserves earlier failures and results.
