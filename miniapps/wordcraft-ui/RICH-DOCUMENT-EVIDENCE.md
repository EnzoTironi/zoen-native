# Rich Word document work in progress (#583)

The successor uses WordCraft `d1b0bfc3f9dd83126b162a42b667facecb003b7f` and its actual Document/Session/DOCX/PDF implementations. Core shares the document/codec contract without importing the editor, layout or fonts. Manual snapshots serialize the actual rich model. A writer/readback that changes the model or any original package part is refused.

Closed agent tools add `wordcraft_edit_range` and `wordcraft_format_range`. Read returns body paragraphs, runs and styles alongside the exact head. The same typed operation schema is used by native review and the worker; it is not an arbitrary command dispatcher. Native permissions, encryption, review, CAS and durable version history retain authority.

The actual live Session keeps Undo when an agent command runs. More importantly, a headless worker's accepted revision arrives through `BrowserBridge.open`: the embedding helper reuses the upstream document reset for transient state, while retaining the bounded actual Undo history. Dirty/pending refresh is refused. Undo creates a local human edit against the latest accepted head; only a new CAS receipt advances it. It never silently rewinds the accepted head/export. Agent typing and subsequent human typing have separate Undo steps.

## Evidence so far

- Actual native WordApp keyboard/canvas and host suite: 39 passed, including typing → headless agent style → refresh → keyboard Undo → new CAS head.
- The same host batch includes complete rich model, metadata, styles, immutable proposals, retry, rollback, current-head Undo/Redo, cold DOCX and PDF.
- Core/miniapps Word batch: 21 passed, including exact owner review, altered intent refusal, agent attribution, durable/idempotent receipts, revocation, read escalation and cross-format refusal.
- The previous higher-head path produced a real failing Undo assertion before the fix. Execution was 0.02 seconds; source/guard/compiler setup failures are not that regression.
- Independent ZIP/XML and pypdf inspection verified Heading1, 18 pt, `204080`, centered paragraph, and both expected text lines. The PDF contains the corresponding 18 pt and RGB operators.
- Core/shared profile Clippy pedantic/performance with all-targets and warnings denied passed. Host all-target pedantic/performance validation passed too; no lint suppression was added for the new code.

Local proof directory: `/Users/enzotironi/.codex/builds/zoen-word-rich-document-v581`. Exported DOCX: 6,554 bytes, SHA-256 `339c065024d34ce656edc908b15a48bb646d69475b5532f50819f5b3ebcd98f7`. PDF: 11,819 bytes, SHA-256 `0a145757294291006729c7c7af914f1f66224665498d09539ed1863a41e92389`.

## Remaining acceptance and limits

The rich browser package is now built normally, inside the existing asset caps, and its exact bytes are pinned. Actual WebKit (normal and press-focus-refusal) and Chromium exercised trusted touch, held-ACK typing, manual italic formatting, a real headless agent formatting proposal, official same-Session refresh, keyboard Undo as a new human version, DOCX/PDF export and cold reopen with zero writes. Their canonical port is synthetic; native Core review/history is a separate proof. Native keyboard #557 remains a separate pending acceptance for this package; desktop WebKit or native egui tests do not prove iOS software keyboard presentation. The normal WASI release is built; the current final signed-tier runtime result is tracked in #583 and PR #589.

The managed profile remains bounded: 512 KiB package, 32 parts, 1 MiB per inflated part, 4 MiB total, 32 KiB body text. Imports must survive the pinned codec exactly; opaque parts/external relationships are refused. Range tools address ordered UTF-8 codepoint boundaries in regular body paragraphs, and refuse ranges crossing tables or other blocks. This is not a claim of all-format Word compatibility, unrestricted editing commands or complete WordCraft feature parity.

Legacy compatibility: a real signed plain-text-v1 ItemCreated with encrypted FileDoc (head1,6397 bytes,SHA0a436…) passed read without migration/write, rich CASv2 on the same item, immutable old bytes/metadata, cold unlock/read and durable retry. The read DTO derives engine_revision from the current registry (still the same pinned d1b upstream); neither the current Core reader nor frontend gate requires replacing old profile metadata.

Read envelopes remain 64 KiB. Ordinary rich paragraphs reference UTF-8 ranges in the one full legacy text rather than duplicating it. If detail cannot fit, an explicit richDetails omission/limit indicator retains paragraph/run/style counts; missing details never mean absent styles. Text is never truncated. A text whose escaping alone cannot fit is explicitly refused while its accepted document/export remains intact.

The official SDK consumer now queues Word head notifications through `refresh_document`.
It drains admitted input before checking draft/pending state, verifies canonical chunks
and authority, then calls the same browser binding. Dirty/pending refresh refuses
without saving or replacing the draft; retirement prevents late reopen/publication.
Five SDK orchestration tests passed with a behavioral engine port. Actual new WASM consumer execution passed separately in WebKit and Chromium. A causal SDK regression also proves that an own/stale head notification must not block newer human input; the observed immutable version is checked before requesting pause.

The normal rich browser release passed in 450.9 s with all Rust producer inputs
unchanged. wasm-bindgen 0.2.129 produced 46,546,230-byte WASM (SHA-256
`e6771bc5d939fff1057d58b78d786611a5975c7e1b6f7d7b8983ceaf6d2dea1f`),
inside the existing 48 MiB cap. WASI release also passed in 478.5 s with unchanged producer inputs; its worker is 35,971,597 bytes, SHA-256 `3e41a135c1faeb042071cbeddd23f76652836f4c032252b104787a9c73a62370`. Native acceptance is still pending.
Existing upstream formatting controls are now exposed (`focus_mode=false`); the
short empty Web page and touch/accessibility usability still require iPhone review.

The final broader app check passed types, lint and format; 2,361 of 2,362 tests
passed initially. The sole failure was an absent ignored generated Swift file,
before the test could run. That exact test then passed with a read-only existing
artifact whose exercised CoreError enum is byte-identical to the verified v586
build. The final affected SDK/history/lifecycle batch passed 28 tests. The configured
new browser resource passed the native miniapps Word batch. CI's stale reviewed
producer/Cargo pins were refreshed without changing job-selection rules; the full
Rust lane and source/embedded-resource negatives remain in force.

## Native keyboard follow-up (#557), v596

The v594 iPhone build reached caret/DOM focus on its first real paper tap, but
showed no software keyboard; another field in the same cohort did show it.
A real WebKit fault regression then refused canvas focus without focusing the
input: the old package lost textual admission; the successor calls the real
text agent within the trusted release, with zero writes and the same head/hash.
The existing primary-contact owner now preserves logical focus through its
press/repaints/release. Cancel, block, secondary contacts and untrusted starts
cannot retain that ownership. Current owner/IME checks still control activation.
There is no WebView keyboard-policy override, injected input focus or load/reopen
keyboard request. This fault model does not establish the native WK policy.

The browser successor is 46,546,380 bytes, SHA-256
`68cc2b6c54417654190f7dfbd13ae0c40dc7d7e46828a0e4ebabf1e89be79e5b`;
JavaScript and HTML are byte-identical to v581. WASI's 30 source inputs and worker
are unchanged. Native first-touch software keyboard acceptance remains pending.
Exact private receipts: `zoen-word-native-gesture-v596` (preserved baseline RED,
first candidate RED, final actual WebKit/Chromium and native UI results).
