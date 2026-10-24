# Rich Word document work in progress (#583)

The successor uses WordCraft `d1b0bfc3f9dd83126b162a42b667facecb003b7f` and its actual Document/Session/DOCX/PDF implementations. Core shares the document/codec contract without importing the editor, layout or fonts. Manual snapshots serialize the actual rich model. A writer/readback that changes the model or any original package part is refused.

Closed agent tools add `wordcraft_edit_range` and `wordcraft_format_range`. Read returns body paragraphs, runs and styles alongside the exact head. The same typed operation schema is used by native review and the worker; it is not an arbitrary command dispatcher. Native permissions, encryption, review, CAS and durable version history retain authority.

The actual live Session keeps Undo when an agent command runs. More importantly, a headless worker's accepted revision arrives through `BrowserBridge.open`: the embedding helper reuses the upstream document reset for transient state, while retaining the bounded actual Undo history. Dirty/pending refresh is refused. Undo creates a local human edit against the latest accepted head; only a new CAS receipt advances it. It never silently rewinds the accepted head/export. Agent typing and subsequent human typing have separate Undo steps.

## Evidence so far

- Actual native WordApp keyboard/canvas suite: 37 passed, including typing → headless agent style → refresh → keyboard Undo → new CAS head.
- Native host suite: 20 passed, including complete rich model, metadata, styles, immutable proposals, retry, rollback, current-head Undo/Redo, cold DOCX and PDF.
- Core/miniapps Word batch: 21 passed, including exact owner review, altered intent refusal, agent attribution, durable/idempotent receipts, revocation, read escalation and cross-format refusal.
- The previous higher-head path produced a real failing Undo assertion before the fix. Execution was 0.02 seconds; source/guard/compiler setup failures are not that regression.
- Independent ZIP/XML and pypdf inspection verified Heading1, 18 pt, `204080`, centered paragraph, and both expected text lines. The PDF contains the corresponding 18 pt and RGB operators.
- Clippy pedantic/performance and warning-denied validation is being completed on the affected packages; no lint suppression is added for the new code.

Local proof directory: `/Users/enzotironi/.codex/builds/zoen-word-rich-document-v581`. Exported DOCX: 6,554 bytes, SHA-256 `339c065024d34ce656edc908b15a48bb646d69475b5532f50819f5b3ebcd98f7`. PDF: 11,819 bytes, SHA-256 `0a145757294291006729c7c7af914f1f66224665498d09539ed1863a41e92389`.

## Remaining acceptance and limits

This source delivery does not qualify a new browser/worker package or the iPhone journey. Existing pinned assets remain unchanged until normal cross-compilation, package qualification and review. Native keyboard #557 remains a separate failing acceptance; desktop WebKit or native egui tests do not prove software keyboard presentation on iOS.

The managed profile remains bounded: 512 KiB package, 32 parts, 1 MiB per inflated part, 4 MiB total, 32 KiB body text. Imports must survive the pinned codec exactly; opaque parts/external relationships are refused. Range tools address ordered UTF-8 codepoint boundaries in regular body paragraphs, and refuse ranges crossing tables or other blocks. This is not a claim of all-format Word compatibility, unrestricted editing commands or complete WordCraft feature parity.
