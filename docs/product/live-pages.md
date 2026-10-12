# Live pages: behavior and completion contract

Updated 12 October 2026 from the user's ChatGPT Space reference, the available Pages tool contracts and Zoen's editor source. [OpenAI's feature page](https://chatgpt.com/pt-BR/features/space/) describes editable pages with collaborators, comments, interactive content, connected context and recurring updates. The user also expects Notion's familiar [block-editing workflow](https://www.notion.com/help/writing-and-editing-basics), [collaboration](https://www.notion.com/help/collaborate-within-a-workspace) and [sharing controls](https://www.notion.com/help/sharing-and-permissions).

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

Native release acceptance also follows the [HIG matrix](hig-compliance.md), including VoiceOver, keyboard, undo/IME and text enlargement.

Current verified baseline: Main `ff5b519` includes shared sessions, typed Plan consumers, negative-cost refusal, offline legacy compatibility, validated cache repair, native/web consolidation, keyboard/motion controls and private authenticated history prefixes. [Complete PR 71 CI](https://github.com/EnzoTironi/zoen-native/actions/runs/38309704131) passes 573 standard tests, including all 198 actual FFI cases; tested and merged trees match. The merged client packages Core193. A separate Core216 native composition passes three Debug builds and bounded menu/keyboard/VoiceOver checks. Actual owned-demo Mac journeys retain exact cents, save/cold reopen/removal/restore and TextKit typing/undo/Unicode save/restore proof. Change events open the same Plan inspector by keyboard. The Plan presents typed blocks with legacy controls before cutover. Scoped canonical decisions pass all 216 local and hosted FFI cases, but PR 70's full hosted suite fails a separate file-import journey. Shared-main integration must resolve that failure and the newly identified preflight/mutation race through atomic Core guards. Canonical writers/history/restore, full block UX and Android/browser/two-device workflows remain open. See [native Page evidence](https://github.com/EnzoTironi/zoen-native/pull/67), [keyboard evidence](https://github.com/EnzoTironi/zoen-native/pull/69) and the [roadmap](../roadmap-status.md).

## Collaboration and agent editing

People with edit access can edit concurrently. Comments anchor to a block or exact selection and support replies, resolution and mentions, following the [Notion commenting workflow](https://www.notion.com/help/comments-mentions-and-reminders). Sharing a page grants access to that page and authorized resources; private chats, personal memory and connector credentials retain their own access controls. A page link does not grant access to an independently protected file or source. See the [ChatGPT Space sharing behavior](https://chatgpt.com/pt-BR/features/space/) and [Notion permissions](https://www.notion.com/help/sharing-and-permissions).

Agents read a bounded page snapshot with exact block IDs, hashes and revision information. Literal edits require an observed hash and an unambiguous original span. Structural changes use explicit insert, move, replace or delete operations. Atomic changes either apply completely or return a conflict; partial edits return a result for every requested operation. An uncertain agent save requires readback and reconciliation before issuing a different mutation. These required behaviors follow the inspected Pages tool contracts; Zoen must implement them through its own signed events and Loro documents. The current editor does not yet expose this complete guarded agent-editing contract.

An agent is an authorized conversation member. Every proposed edit passes the same grants/approval checks as other tools. Suggestions stay visible until approved, applied changes identify their author, and edits cannot silently change sharing or permissions. Preserve concurrent human edits and show a useful conflict/review state.

## Live updates

A live block names its source, refresh policy, responsible member/agent and last successful update. Support manual refresh, source-change refresh and schedules. Use one explicit controller for a page's maintained sections; pause/resume has visible state and stops dependent workers. Refresh failures retain the last good content and explain what needs attention.

Only write a new page version when the materialized content changes. Record source provenance and definition versions. Connector credentials stay in the owner's runtime. The encrypted relay does not interpret page content or refresh sources.

## Current implementation boundary

The [session contract](../dev/live-page-session.md) defines observed contexts, exact retries, encrypted drafts and the retained older editing adapter. `plan_page()` is an additive typed read; it does not activate canonical Plan mutation. Its `source` distinguishes an authenticated document session from this account's signed durable pending legacy preview. `legacy_edit_admitted` preserves the existing writer boundary rather than granting new authority.

| Capability | Current implementation/evidence | Remaining proof |
| --- | --- | --- |
| Native editing | `PageScreen`, `PageEditorController`, `PageTextView` and `FormatBar`; stable block DTOs, observed-context edits, raw/display mapping and encrypted checkpoints. Actual Mac TextKit typing/undo, Unicode paste, save/cold reopen, read-only preview and new-version restoration pass. Scoped history Escape retains its editor. | IME, full selection/viewport and interruption journeys, physical iOS/Release, accessible slash menu and all supported block types. |
| Typed Plans | `PlanPageScreen` preserves original block order, exact text/UTF-16 marks, stable CRDT IDs and signed cents. Current Mac legacy-compatible field edits/save/reopen/remove/restore pass. | Canonical writers, history/restore migration, strict approval replay, reader/removed-member UI and independent-device convergence. |
| Versions and persistence | Shared signed history, encrypted journal receipts, retry/restore fences and atomic publication. | Release Keychain/file protection, encrypted two-device editing and recovery/convergence. |
| Live data and agent sections | [ADR 0041](../adr/0041-live-pages.md) and the required behavior above. | Refresh bindings, comments, permissions, proposals, scheduled controller lifecycle and source/definition provenance. |
| Web/Android parity | Themed web shell/sample preview and Android editor source retain separate revision evidence. | Current Android ABI/device editor and authenticated browser collaboration/editing/sharing. |

Older codec, Core60/Core134, editor checkpoint and app journeys retain their original source scope in [dated history](../history/roadmap-2026-10-12-before-consolidation.md). Compiler or standalone Foundation tests do not complete release/HIG, independent-device or live provider acceptance.

## Local retry and restore contract

The local candidate's `page_apply_from` accepts a stable mutation ID, opaque observed context, block order and changed blocks. It persists the encrypted device draft and receipt before acknowledging the edit. An identical retry returns the recorded receipt without applying the mutation twice; reusing an ID for a different request is refused. Every actual local revision is submitted, including an undo whose target matches the old displayed baseline. Comparing only the visible text would miss an earlier edit accepted before its response was lost.

The response separates the editor's own applied context from the current merged page, which can contain other edits. A caller continuing its earlier branch uses the applied context; a caller displaying the merged page adopts that page's blocks and context together. Changing the baseline or context requires a fresh mutation ID. Own-edit undo/redo must preserve unseen edits. Receipt retention is bounded; an expired or invalid context requires rereading and reconciling the draft.

Restoring a version durably invalidates earlier edit contexts. Retained receipts still acknowledge the original edit, but their continuation context cannot overwrite the restored page. Reader or removed-member permissions are checked again before edits or retries. Draft journals are encrypted with a device-derived key and scoped to the identity, conversation and page; they are excluded from portable backups. Saved versions and encrypted backup recovery do not promise recovery of an unsaved draft after that device is lost.

Local core tests cover file-backed reopen, failure/retry, own-edit undo/redo, duplicate-ID atomicity, permission refusal, restore fences and pending/failed acknowledgement. Native input/viewport behavior and independent-device convergence still require runtime journeys. Full comments, nested-page organization, sharing, live bindings and guarded agent tools remain unfinished.

The next implementation must test two independent members editing different and overlapping blocks, offline changes followed by reconnect, an agent suggestion with approve/deny/undo, a revoked editor, live refresh that changes nothing, failed refresh and nested-page moves. Attach actual native/browser editing videos to its PR. A static page mock does not complete these gates.
