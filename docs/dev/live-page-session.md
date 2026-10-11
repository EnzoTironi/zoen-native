# Live Page document session

Binary Pages now use one session for saved history, local edits, encrypted draft receipts, recovery and publication. The new editing API carries the state the editor observed instead of replacing whichever blocks are current when the call arrives.

## Client contract

1. Read `page(item_id)`. Keep its `edit_context` with the blocks the editor actually displays.
2. Allocate one mutation ID for an edit and call `page_apply_from` with that ID, the observed context, complete block order and changed blocks.
3. After an uncertain response, repeat the exact ID, context and edit. A retained receipt returns `replayed = true` without applying the edit twice.
4. Display the returned `page`, which can include other edits. `applied_content_hash` and `applied_edit_context` identify this editor's accepted operation; they can differ from the merged page.
5. Publish with `page_commit`. Show `pending_sync` and `save_error`; a local version alone does not prove acceptance by a remote relay.

A context is bound to its actor, space, item and creation. After restore, an unaccepted edit from an older context returns `Stale`. An already accepted exact retry can still return its retained receipt, without applying it again; its current `page` remains restored. Receipt retention is bounded to 512 entries; an evicted generation returns `Stale`. Re-read and reconcile the person's intent rather than assigning a new ID to an obsolete full document.

The older `page_apply` entry point remains available for current clients. Its adapter reads the current context under the Engine lock. It cannot carry the context an older client previously observed. Native editors still using it must migrate to `page_apply_from` before their concurrent-edit journey is considered complete.

## Persistence and recovery

The session keeps a verified saved document and the local document. Repeated reads authenticate the current encrypted journal before reusing cached state. Reload changes the cache epoch, so saved payloads must be verified again.

Journal recovery never replaces unreadable ciphertext with an empty draft. A cold reader can still see the saved page while editing is disabled. Publication stages the blob, signed event, outbox and encrypted journal marker in one SQLite savepoint. Failure rolls back the actual writes and retains the recoverable draft. A refused save keeps its error and unsaved indication visible.

Existing Binary Page Loro content, preference and database keys, and the `Storage(message)` FFI field are preserved. Regenerate Swift/Kotlin bindings together with the core library: the Page record gains fields and the observed-context API is new. The user-facing name is Zoen; persisted `roda` identifiers are not renamed by this change.

## Plan admission

The typed Plan document, authenticated timeline and tagged journal are foundations for a shared Live Page editor. Public Page APIs still reject Plan Items. The Plan consumer candidate replaces current card, item and search projections without activating the public Plan editor.

After valid cutover, consumers read the session's saved document and accepted version, author and timestamp. They exclude private drafts and refused later whole-plan writes while retaining those writes in history. Before cutover, existing offline legacy previews preserve their exact bytes. Search indexes the accepted document's blocks and typed rows; it does not introduce another editable Plan store.

Legacy edit, toggle, add, remove, restore and linked approval mutations refuse before changing storage once cutover is saved or pending. A poll that would add a legacy Plan row also refuses before closing or posting its result. Existing approval display compares the first matching legacy row ID's exact text and cents; completion alone does not change the hash. The stricter C4 proposal, writer, space and transactional approval implementation must be reconciled separately before integrating that backend stack.

Totals use checked integer addition. Cards report an unavailable total on overflow while preserving row counts and stored history. The older `PlanDto` requires an `i64` total, so `item()` returns an explicit error for overflow instead of zero, clamping or panic. A typed optional-total client DTO and canonical history/restore views are still required before public admission. Historical negative cents, including `i64::MIN`, remain readable.

Before admission, finish typed client DTOs, canonical writers and historical views, restore policy, mixed-version writer refusal, native controls and actual two-device/provider journeys. Plan financial metadata must never be flattened into ordinary paragraph text.

## Verification scope

PR 60's shared-session foundation is merged as main `fa4eed8`; its Linux run passed 61 document, 7 compatibility and 120 FFI tests. The subsequent Plan consumer core candidate `610895f` passed formatting, all-target FFI Clippy and the full 127-test FFI suite on 11 October 2026. Its seven added consumer tests first ran against unchanged main production source: one passed and six failed. The fixed source retained all 120 original tests and passed all seven new cases, with no tests ignored or filtered. The ordinary unoptimized profile used the default stack and the existing resource/time guards.

The standalone public API evidence uses an owned local SQLite database and two editor contexts for one actor. It is separate from native app footage, relay/provider execution, full HIG acceptance and billion-user capacity evidence. Those gates remain open in the [roadmap](../roadmap-status.md).
