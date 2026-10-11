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

The typed Plan document, authenticated timeline and tagged journal are foundations for a shared Live Page editor. Public Page APIs still reject Plan Items. This change does not activate Plan writers or replace current Plan readers.

Before admission, migrate the canonical card/item/search projections, approvals and financial hashes, legacy mutations and historical views. Finish typed client DTOs, restore policy, mixed-version writer refusal, native controls and actual two-device/provider journeys. Plan financial metadata must never be flattened into ordinary paragraph text.

## Verification scope

The isolated candidate based on main `7cad5b1` passed formatting, all-target Clippy, 61 document tests, 7 type compatibility tests and all 120 FFI tests on 11 October 2026. No tests were ignored or filtered. The ordinary unoptimized profile used the default stack and the existing resource/time guards.

The standalone public API evidence uses an owned local SQLite database and two editor contexts for one actor. It is separate from native app footage, relay/provider execution, full HIG acceptance and billion-user capacity evidence. Those gates remain open in the [roadmap](../roadmap-status.md).
