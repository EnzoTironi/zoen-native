# Live pages: behavior and completion contract

Updated 10 October 2026 from the user's ChatGPT Space reference, the available Pages tool contracts and Zoen's current editor. [OpenAI's feature page](https://chatgpt.com/pt-BR/features/space/) describes editable pages with collaborators, comments, interactive content, connected context and recurring updates. The user also expects the familiar block-editing workflow of Notion.

A page belongs to a conversation or the person's own files. Groups and communities remain in Chats; pages do not introduce another conversation inbox.

## Editing and organization

- Edit formatted blocks directly. Support headings, paragraphs, tasks, lists, quotes, code, dividers, images, tables and links to other pages.
- Use `/` to insert or change a block; support keyboard navigation and an accessible menu on mobile and desktop.
- Add, move, duplicate and delete blocks with stable IDs. Preserve selection and unsaved edits while another member changes a different block.
- Organize nested pages with breadcrumbs and page links. Renaming or moving a page preserves its identity, links and history.
- Show save state, offline pending changes, author attribution and versions. Undo creates a new change without deleting history.

## Collaboration and agent editing

People with edit access can edit concurrently. Comments anchor to a block or exact selection and support replies, resolution and mentions. Sharing a page grants access to that page and authorized resources; it does not expose private chats, personal memory or connector credentials.

Agents read a bounded page snapshot with exact block IDs, hashes and revision information. Literal edits require an observed hash and an unambiguous original span. Structural changes use explicit insert, move, replace or delete operations. Atomic changes either apply completely or return a conflict; partial edits return a result for every requested operation. An uncertain save requires readback before retrying. These required behaviors follow the inspected Pages tool contracts; Zoen must implement them through its own signed events and Loro documents. The current editor does not yet expose this complete guarded agent-editing contract.

An agent is an authorized conversation member. Every proposed edit passes the same grants/approval checks as other tools. Suggestions stay visible until approved, applied changes identify their author, and edits cannot silently change sharing or permissions. Preserve concurrent human edits and show a useful conflict/review state.

## Live updates

A live block names its source, refresh policy, responsible member/agent and last successful update. Support manual refresh, source-change refresh and schedules. Use one explicit controller for a page's maintained sections; pause/resume has visible state and stops dependent workers. Refresh failures retain the last good content and explain what needs attention.

Only write a new page version when the materialized content changes. Record source provenance and definition versions. Connector credentials stay in the owner's runtime. The encrypted relay does not interpret page content or refresh sources.

## Current implementation and gaps

| Capability | Current code | Required proof |
| --- | --- | --- |
| Native formatted editing | `PageScreen`, `PageEditorController`, `PageTextView`, `FormatBar`; stable block DTOs and Loro documents. | Real typing/selection/formatting on iOS and Mac, accessible slash menu and supported block types. |
| Versions and persistence | Signed Item versions; inline updates and encrypted blobs for large page payloads. | Concurrent edits, interruption/relaunch, save conflicts and cross-device convergence. |
| Live data and agent sections | [ADR 0041](../adr/0041-live-pages.md) design. | Refresh bindings, comments, permissions, proposals, scheduled controller lifecycle and provenance. |
| Web/Android parity | Android editor under review; browser shell is a preview. | Authenticated native/browser collaboration and equivalent editing/sharing workflows. |

The next implementation must test two independent members editing different and overlapping blocks, offline changes followed by reconnect, an agent suggestion with approve/deny/undo, a revoked editor, live refresh that changes nothing, failed refresh and nested-page moves. Attach actual native/browser editing videos to its PR. A static page mock does not complete these gates.
