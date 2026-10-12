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

The typed Plan document, authenticated timeline and tagged journal are foundations for a shared Live Page editor. Canonical Page mutation APIs still reject Plan Items; the dedicated `plan_page()` read is merged in PR 64. PR 62’s merged consumers replace current card, item and search projections without activating the public Plan editor.

After valid cutover, consumers read the session's saved document and accepted version, author and timestamp. They exclude private drafts and refused later whole-plan writes while retaining those writes in history. Before cutover, existing offline legacy previews preserve their exact bytes. Search indexes the accepted document's blocks and typed rows; it does not introduce another editable Plan store.

Legacy edit, toggle, add, remove, restore and linked approval mutations refuse before changing storage once cutover is saved or pending. A poll that would add a legacy Plan row also refuses before closing or posting its result. Existing approval display compares the first matching legacy row ID's exact text and cents; completion alone does not change the hash. The stricter C4 proposal, writer, space and transactional approval implementation must be reconciled separately before integrating that backend stack.

Totals accumulate checked `i128` values before converting the final result to `i64`; compensating credits cannot cause a false overflow or make the total depend on row order. Cards report an unavailable total on overflow while preserving row counts and stored history. The older `PlanDto` requires an `i64` total, so `item()` returns an explicit error for overflow instead of zero, clamping or panic. The new `plan_page()` client read returns a typed `PlanPageDto` with an optional total. Canonical history/restore views and write admission still require migration. Historical negative cents, including `i64::MIN`, remain readable.

The additive `plan_page()` read uses the same document session and returns closed typed variants for title/budget, summary, section, priced line and prose. Stable CRDT block IDs remain distinct from possibly duplicated legacy row IDs. Exact raw text, UTF-16 marks, signed integer cents, heading style and image titles survive the client boundary. The optional total is computed from the returned rows; a mathematical value outside `i64` stays unavailable. The version is the last accepted shared version, while `unsaved` labels this account’s private draft. Cards and search continue to read the accepted shadow. This endpoint exposes no editing context or write capability. Native/Android/web binding and presentation journeys remain required.

Before write admission, finish canonical writers and historical views, restore policy, mixed-version writer refusal, strict C4 linked-request live/replay, native controls and actual two-device/provider journeys. Plan financial metadata must never be flattened into ordinary paragraph text.

## Verification scope

PR 60's shared-session foundation is merged as main `fa4eed8`; its Linux run passed 61 document, 7 compatibility and 120 FFI tests. The initial Plan consumer candidate `610895f` passed the full 127-test FFI suite. The resulting consumer implementation merged as main `32e0415`. Fresh main Linux CI passed 476 tests across 65 standard harness summaries, including all 128 FFI tests, with zero failures and one existing helper ignored outside FFI. Its seven added consumer tests first ran against unchanged main production source: one passed and six failed. The eighth consumer test first failed through the card API against the earlier consumer candidate; it then passed legacy and canonical reads, exact rows, promotion and reload for compensating credits at both integer limits in different orders. The fixed source retained all 120 original tests and all seven earlier consumer tests, with no tests ignored or filtered. The ordinary unoptimized profile used the default stack and the existing resource/time guards.

The standalone public API evidence uses an owned local SQLite database and two editor contexts for one actor. It is separate from native app footage, relay/provider execution, full HIG acceptance and billion-user capacity evidence. Those gates remain open in the [roadmap](../roadmap-status.md).

The additive typed Plan client read subsequently passed formatting, all-target FFI Clippy and all 132 FFI tests, with zero failures, ignored or filtered tests, plus the empty doc-test target. Four new journeys exercise exact duplicate IDs/text, all prose variants and UTF-16 marks through authenticated cutover, private-draft separation, extreme cents and order-independent credit cancellation. This is local core evidence; current generated client bindings and app workflows remain separate.

PR 65 merged negative legacy-addition validation as `04fdb7a`. Its complete Linux run passes 482 tests, including all 134 FFI cases; the tested tree matches the resulting main. The SDK was regenerated and full arm64 Mac, iOS simulator and generic iOS device Debug apps compiled after correcting C/C++ library deployment targets to version 26. These builds do not establish current runtime, Release, physical minimum-OS or HIG acceptance. Ten Swift presentation/API cases independently exercise exact text/cents, duplicate-row addressing and real generated Core reads.

The native Plan replacement remains unshipped. Existing session `can_edit` blocks pending legacy intent, while old legacy writers allow successive offline edits. A pending creation may also lack an authenticated session seed despite having a readable local legacy preview. Compatibility work must preserve those offline flows without manufacturing an authenticated manifest, opening canonical writes or exposing a second editable store. A presentation/admission flag is not financial authorization.

Backend integration update (12 October 2026, 17:15 UTC): PR 63 is merged as Main `0b349d3` after complete 558-pass Linux CI, including 183 actual Core tests. The compiled native Core134 baseline and its ten Swift/API cases retain their separate scope. The offline compatibility candidate has not passed its full suite yet; the first run exposed invalid fixture input and a real damaged-media import recovery bug. The corrected candidate is being retested before generating new bindings and validating the replacement Plan screen.
