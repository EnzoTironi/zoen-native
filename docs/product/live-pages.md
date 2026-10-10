# Live pages: behavior and completion contract

Updated 10 October 2026 from the user's ChatGPT Space reference, the available Pages tool contracts and Zoen's editor source. [OpenAI's feature page](https://chatgpt.com/pt-BR/features/space/) describes editable pages with collaborators, comments, interactive content, connected context and recurring updates. The user also expects Notion's familiar [block-editing workflow](https://www.notion.com/help/writing-and-editing-basics), [collaboration](https://www.notion.com/help/collaborate-within-a-workspace) and [sharing controls](https://www.notion.com/help/sharing-and-permissions).

A page belongs to a conversation or the person's own files. Groups and communities remain in Chats; pages do not introduce another conversation inbox.

## Editing and organization

- Edit formatted blocks directly. Support headings, paragraphs, tasks, lists, quotes, code, dividers, images, tables and links to other pages.
- Use `/` to insert or change a block; support keyboard navigation and an accessible menu on mobile and desktop.
- Add, move, duplicate and delete blocks with stable IDs. Preserve selection and unsaved edits while another member changes a different block.
- Organize nested pages with breadcrumbs and page links. Renaming or moving a page preserves its identity, links and history.
- Show save state, offline pending changes, author attribution and versions. Undo creates a new change without deleting history.

Save state distinguishes a local draft, a version queued for sync, relay confirmation and a refused version that needs attention. A successful local edit receipt is not relay confirmation. Formatting controls and delayed actions stop mutating when edit permission is removed. Preserve IME composition, the selection and the visible block while merging remote changes.

## Plans use the same Live Page

A plan appears as the same compact card as a mini-app in chat, pinned widgets and files. Opening the card uses the common item presentation and the Live Page editor. The plan keeps its existing identity, links, versions, budget and approval references. There is one editable document. MCP can expose that document through the same authorized tools; it does not create another copy or store.

Title, prose, sections and task/expense rows are blocks. Cost, completion and budget are independent typed fields, so changing text concurrently with a cost or checkbox does not overwrite the other edit. Legacy IDs, exact text and integer cents survive promotion. A move preserves the row and its text container. Deleting a section while another member inserts a row retains that row in a visible recovery group. An overflowing total shows an explicit unavailable total without losing either valid edit.

Requests stay bound to exact line text and cost. Changing either requires a new approval; formatting or completion alone does not change the financial hash. Edits, restores, polls and authorized agents use the same document writer, permissions, retry receipts and history. Old whole-plan writes after cutover remain recoverable and visibly refused instead of replacing newer page edits.

Current state: PR 44 implements the shared card pattern, verified in the actual browser preview and Mac demo. It still opens the existing plan editor. The isolated Page/Loro model and bounded inspectable codec passed 23 Plan tests, all 36 document tests and Clippy under Rust 1.99.0. Tests cover canonical replay, concurrent fields, exact IDs/text/cents, conflicting operation IDs, Unicode slices, retries and invalid payload refusal. This library codec is not yet the production Page wire. Signed migration/cutover, semantic receipt acceptance, native typed fields/checkpoints, caller migration, large-history handling and cross-platform journeys are required before enabling Live Page plans.

## Collaboration and agent editing

People with edit access can edit concurrently. Comments anchor to a block or exact selection and support replies, resolution and mentions, following the [Notion commenting workflow](https://www.notion.com/help/comments-mentions-and-reminders). Sharing a page grants access to that page and authorized resources; private chats, personal memory and connector credentials retain their own access controls. A page link does not grant access to an independently protected file or source. See the [ChatGPT Space sharing behavior](https://chatgpt.com/pt-BR/features/space/) and [Notion permissions](https://www.notion.com/help/sharing-and-permissions).

Agents read a bounded page snapshot with exact block IDs, hashes and revision information. Literal edits require an observed hash and an unambiguous original span. Structural changes use explicit insert, move, replace or delete operations. Atomic changes either apply completely or return a conflict; partial edits return a result for every requested operation. An uncertain agent save requires readback and reconciliation before issuing a different mutation. These required behaviors follow the inspected Pages tool contracts; Zoen must implement them through its own signed events and Loro documents. The current editor does not yet expose this complete guarded agent-editing contract.

An agent is an authorized conversation member. Every proposed edit passes the same grants/approval checks as other tools. Suggestions stay visible until approved, applied changes identify their author, and edits cannot silently change sharing or permissions. Preserve concurrent human edits and show a useful conflict/review state.

## Live updates

A live block names its source, refresh policy, responsible member/agent and last successful update. Support manual refresh, source-change refresh and schedules. Use one explicit controller for a page's maintained sections; pause/resume has visible state and stops dependent workers. Refresh failures retain the last good content and explain what needs attention.

Only write a new page version when the materialized content changes. Record source provenance and definition versions. Connector credentials stay in the owner's runtime. The encrypted relay does not interpret page content or refresh sources.

## Current implementation and gaps

| Capability | Current code | Required proof |
| --- | --- | --- |
| Native formatted editing | `PageScreen`, `PageEditorController`, `PageTextView`, `FormatBar`; stable block DTOs and Loro documents. The local Apple candidate gates read-only mutations and maps selection/viewport through multiple edit regions. | Real typing, IME composition, selection, formatting and interruption/relaunch on iOS and Mac; accessible slash menu and supported block types. |
| Versions and persistence | Signed Item versions; inline updates and encrypted blobs for large page payloads. The local candidate adds encrypted device draft/receipt persistence, exact retry and restore fencing. | Current native runtime, encrypted two-device edits, offline/relaunch reconciliation and cross-device convergence. |
| Live data and agent sections | [ADR 0041](../adr/0041-live-pages.md) design. | Refresh bindings, comments, permissions, proposals, scheduled controller lifecycle and provenance. |
| Web/Android parity | Android editor changes and generated bindings are under local validation; browser shell remains a sample-data preview. | Native Android editor runtime, authenticated browser collaboration and equivalent editing/sharing workflows. |

The integrated baseline is `main` at `f277c804`, including the reviewed model gateway merge. The local editing candidate is core `aaf4315` plus native `021c42c`, with Android `31f2ce0` merged locally and the CLI dependency correction at `1846f1f`; it is not deployed. Local verification passed 13 document tests and all 80 FFI tests. Current generated bindings and new Page UI compiled in Gradle and passed 95 unit tests across 15 suites, including 13 editor/offset and 10 coordinator cases, with no failures, errors or skips. Apple source parsed and 12 cases exercised its actual offset-mapping function. Swift/Kotlin binding generation and `cargo check --locked --all-targets` succeeded. Gradle skipped only `buildRodaCore`, so Android ABI/JNI, Keystore and device UI remain unverified.

The committed Mac candidate's clean build exited 0 and produced app bundles. The subsequent C/C++ deployment fix rebuilt all 1,152 archive objects for macOS 26 or earlier and the Mac app rebuilt without newer-target warnings. Runtime on macOS 26 remains unverified. The uncommitted Reader/history/debug-isolation follow-up built successfully. A manual Mac journey verified editing, visible version history and restoration as a new version. The encrypted Apple keystroke checkpoint built successfully on Mac and passed ten journeys using production CryptoKit, filesystem and core. Nine use a memory vault; one recreates the actual native development vault and recovers its encrypted checkpoint. An unreadable draft can be retained as an encrypted file or explicitly discarded without touching other drafts, and a stale confirmation cannot delete a newer edit. The actual manual Mac journey completed save/history/restore and two quit/reopen cycles, ending at v5 with exact Unicode text preserved. The certified XCTest runner builds but stalls before cases; developer security settings were not changed. Release Keychain and iOS file protection remain unverified; real IME, Reader remote updates and other platforms still require runtime proof. The [roadmap ledger](../roadmap-status.md) records provenance and the remaining service checks.

## Local retry and restore contract

The local candidate's `page_apply_from` accepts a stable mutation ID, opaque observed context, block order and changed blocks. It persists the encrypted device draft and receipt before acknowledging the edit. An identical retry returns the recorded receipt without applying the mutation twice; reusing an ID for a different request is refused. Every actual local revision is submitted, including an undo whose target matches the old displayed baseline. Comparing only the visible text would miss an earlier edit accepted before its response was lost.

The response separates the editor's own applied context from the current merged page, which can contain other edits. A caller continuing its earlier branch uses the applied context; a caller displaying the merged page adopts that page's blocks and context together. Changing the baseline or context requires a fresh mutation ID. Own-edit undo/redo must preserve unseen edits. Receipt retention is bounded; an expired or invalid context requires rereading and reconciling the draft.

Restoring a version durably invalidates earlier edit contexts. Retained receipts still acknowledge the original edit, but their continuation context cannot overwrite the restored page. Reader or removed-member permissions are checked again before edits or retries. Draft journals are encrypted with a device-derived key and scoped to the identity, conversation and page; they are excluded from portable backups. Saved versions and encrypted backup recovery do not promise recovery of an unsaved draft after that device is lost.

Local core tests cover file-backed reopen, failure/retry, own-edit undo/redo, duplicate-ID atomicity, permission refusal, restore fences and pending/failed acknowledgement. Native input/viewport behavior and independent-device convergence still require runtime journeys. Full comments, nested-page organization, sharing, live bindings and guarded agent tools remain unfinished.

The next implementation must test two independent members editing different and overlapping blocks, offline changes followed by reconnect, an agent suggestion with approve/deny/undo, a revoked editor, live refresh that changes nothing, failed refresh and nested-page moves. Attach actual native/browser editing videos to its PR. A static page mock does not complete these gates.
