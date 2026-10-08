# ADR 0023: A viewer for every file, an editor for most, and a native WYSIWYG Markdown editor

Status: proposed (research done 2026-10-08; nothing built yet). Companion ADRs: 0024 (live
pages) and 0025 (dynamic UI). Builds on 0007 (encrypted blobs) and 0014 (files are Items).

## Context
Enzo: "Eu quero visualizador e editor para todos os formatos. E para md precisamos de WYSIWYG."

What exists today (origin/main `406515f`):
- **Model.** `ItemKind` is `Plan | Task | Note | App`; `ItemContent` is `Text { text } | Plan |
  App`. A Note is one plain `String` and every edit re-sends the whole text in `ItemVersioned`.
  ADR 0014 plans `File, Note, Skill, Claim, Index, Behavior` with a `path`; not built (M2.6).
- **Bytes.** `MediaRef { sha256, mime, width, height, bytes, key?, blob? }` and the ADR 0007
  pipeline: fresh 32-byte key per attachment, XChaCha20-Poly1305, ciphertext addressed by
  sha256, signed `PUT /v1/blobs/{sha256}`, upload queue and verified downloads
  (`crates/roda-ffi/src/media.rs`). Used for chat backgrounds and profile photos; voice notes
  and files are "remaining". No chunking or resumable upload yet (plan: 4 MB chunks).
- **App.** `FilesView.swift` lists Items grouped by Space ("Folders", "Recent"); `ItemView.swift`
  renders a Plan as cards and a Note as a read-only `Text`. No document picker, no QuickLook,
  no PDFKit, no AVKit, no editor of any kind except `VoiceEditor.swift` (voice notes) and the
  photo framing editor for chat backgrounds. The Files header shows "signed events", which
  breaks the no-jargon rule and goes away with this work.
- **Plans already made.** repensado.md §6 chose Loro (CRDT) + a native `TextEditor` +
  `AttributedString` editor, TipTap only on the web invite page, TextKit 2 "later".
  This ADR keeps Loro and native, and corrects the editor surface (below).

Constraints: everything opens and edits on the device (the relay and S3 only ever hold
ciphertext); proprietary all-rights-reserved license, so no GPL/AGPL/LGPL-static/SSPL code in
the app; 1B-user cost; no jargon in the UI; haptics and motion matter; user-journey tests only.

## Decision

### 1. One file model
- `ItemKind::File` (bytes in a blob) and `ItemKind::Page` (a Zoen document whose content is a
  Loro CRDT, ADR 0024). Both carry `path` (folders are path prefixes, ADR 0014).
- A File version is a new `MediaRef` (same Item, `ItemVersioned`). Large files use 4 MB
  content-defined chunks (FastCDC) encrypted under per-chunk keys derived (HKDF) from the
  file's key, so a new version uploads only changed chunks and dedupe never crosses files or
  people (no convergent encryption: it would leak "who has this file").
- Thumbnails and previews are made on the device (QuickLook thumbnails, `resvg` for SVG,
  first-frame for video), encrypted as their own small blobs, referenced from the version.
- Markdown import/export, CSV/XLSX/DOCX conversion live in the Rust core so device, agents
  and export bundles share one implementation.

### 2. Viewers and editors per format
Native Apple frameworks first; third-party only if MIT/BSD/Apache and maintained. Columns:
**View** (iOS = macOS unless noted) / **Edit** / **Library and license** (checked on GitHub
2026-10-08: LICENSE file and last release or push).

| Family | View | Edit | Library, license, status |
|---|---|---|---|
| Markdown | Zoen page editor in read mode | **Native WYSIWYG** (§3) | comrak BSD-2-Clause (push 2026-10-06), Loro MIT (1.16.4, 2026-09-30) |
| Plain text, code, logs | TextKit 2 view, tree-sitter highlighting; files over 10 MB open read-only, memory-mapped and paged | Same view editable: line numbers, find/replace, indent | tree-sitter MIT (active). Runestone MIT (iOS only, 0.5.2, 2026-03) and CodeEditSourceEditor MIT (macOS only, push 2026-04) are references, not deps: one TextKit 2 view serves both platforms |
| JSON, YAML, TOML, XML | Collapsible tree view + text | Text editor with validation and "Arrumar" (format) | serde_json / serde_yaml (MIT/Apache) in core |
| CSV, TSV | Native grid (lazy rows, frozen header) | Native grid editor on Loro (cell edits, add/remove/move rows and columns, sort, filter) | csv crate (MIT/Unlicense) |
| PDF | PDFKit `PDFView` (thumbnails, search, outline) | Highlight, ink (PencilKit), notes, text, signature, fill forms, reorder/rotate/delete pages, merge | PDFKit + PencilKit (Apple). **MuPDF is AGPL-3.0: excluded** |
| Images: JPEG, PNG, GIF, WebP, HEIC, TIFF | Native zoomable viewer, Live Photo playback | Crop/rotate/straighten, light/color adjustments, filters, markup | Core Image, PhotosUI, PencilKit |
| RAW (DNG, CR3, NEF, ARW…) | Core Image `CIRAWFilter` | Basic develop (exposure, white balance, highlights), export JPEG/HEIC as new version | Apple. **LibRaw is LGPL-2.1/CDDL-1.0: excluded** |
| Video | AVKit `VideoPlayer` | Trim, split, mute, rotate, crop, speed, combine clips | AVFoundation (`AVMutableComposition`, `AVAssetExportSession`). **FFmpeg is LGPL-2.1+/GPL: excluded** |
| Audio | Waveform player | Trim, split, fade, normalize, voice cleanup | AVAudioEngine; extends `VoiceEditor.swift` |
| Word (docx, doc, rtf) | QuickLook | Phase 4: "Editar" converts docx → Page (headings, lists, tables, images, comments), edits natively, exports docx as a new version; fidelity notice when the source has things we can't keep. Full fidelity: "Abrir no Word/Pages" and the returned file comes back as a new version | docx-rs MIT (push 2026-10-05). RTF via `NSAttributedString` |
| Excel (xlsx, xls, ods) | QuickLook (and our grid for xlsx after Phase 3) | Phase 4: native grid with formulas for the common subset; values and formats round-trip; unsupported features (macros, pivots, charts) kept untouched and flagged | umya-spreadsheet MIT (read/write, push 2026-10-05), calamine MIT (fast read, push 2026-09), rust_xlsxwriter Apache-2.0 |
| PowerPoint (pptx, ppt, key) | QuickLook | Not editable in v1: present, annotate as PDF, edit slide text in Phase 5 if demand; "Abrir no Keynote/PowerPoint" round trip | No maintained permissive pptx writer found |
| Apple iWork (pages, numbers, key) | QuickLook | **View only.** No public format or API; "Abrir no Pages/Numbers/Keynote" and the saved file returns as a new version | Apple |
| HTML | WKWebView with no network (content rule list blocks every load), no JS by default, non-persistent data store | Edit as code (text editor), live preview | WebKit |
| ePub | Readium navigator (pagination, fonts, highlights) | Highlights and notes only | readium/swift-toolkit BSD-3-Clause (3.11.0, 2026-07-17) |
| SVG | WKWebView (no network) / resvg thumbnails | Edit as code with live preview | resvg Apache-2.0 (active) |
| 3D (usdz, reality) | QuickLook AR / RealityKit `Model3D` | View only | Apple |
| Archives (zip; also tar, gz, 7z, rar read) | Browse contents as a folder, open any inner file with its viewer | Extract to Items, create zip from a selection | ZIPFoundation MIT (push 2026-09-12); Apple Archive; libarchive BSD-2-Clause for the rest (per-file licenses checked at vendoring) |
| Fonts, calendar (ics), contacts (vcf) | QuickLook, plus "Adicionar ao Calendário/Contatos" | n/a | Apple |
| Anything else | QuickLook if it can; otherwise a file card (name, size, kind) with Share and "Abrir em…" | n/a | Apple |

**QuickLook is the universal floor.** Every file opens: `QLPreviewController` on iOS and
`QLPreviewPanel`/`QLPreviewView` on macOS, fed from a decrypted temporary file in the app's
protected container that is deleted when the preview closes. QuickLook's own markup
(`editingMode = .updateContents`) gives PDF and image annotation on day one, before the
dedicated editors arrive, and each save becomes a new version.

**Excluded as incompatible with the license or with E2E** (verified from LICENSE files):
- ONLYOFFICE Document Server AGPL-3.0;
- MuPDF AGPL-3.0;
- STTextView GPL-3.0 or paid commercial license;
- BlockNote's `xl-*` packages GPL-3.0 (core is MPL-2.0);
- LibRaw LGPL-2.1/CDDL-1.0 and FFmpeg LGPL-2.1+/GPL, because static linking on iOS makes LGPL
  compliance impractical.

Collabora Online is MPL-2.0 (weak copyleft, not GPL), but it is a server that must see the
plaintext document to render and edit it. That breaks E2E no matter what the license says.
Any server-side office suite is out for the same reason.

### 3. Markdown WYSIWYG: a native block editor on TextKit 2, Loro underneath

Options compared:

| | Round-trip fidelity | Feel on iPhone/Mac | Offline / E2E | Collaboration later | Big files | Verdict |
|---|---|---|---|---|---|---|
| **A. Native TextKit 2 view (UITextView/NSTextView) + Loro + comrak** | Lossless for untouched blocks (source kept per block), canonical for edited ones; unknown syntax kept verbatim | Real system selection, magnifier, dictation, Writing Tools, Apple Pencil Scribble, keyboard shortcuts, haptics we control | Fully on device | Loro (MIT, Rust, already in our core plan): rich-text marks, movable trees, history, forks | TextKit 2 lays out only the viewport | **Chosen** |
| B. SwiftUI `TextEditor` + `AttributedString` (iOS 26) | Inline marks only; block structure (`presentationIntent`) is not rendered; no attachments | Very native | On device | Possible | Good | Use for small rich fields (captions, comments), **not** the page editor: no headings/lists/tables/embeds as blocks, no inline views (verified in Apple docs and WWDC25 session 280) |
| C. Web editor in WKWebView: Milkdown (MIT, 7.22.2, 2026-09-23), TipTap (MIT, 3.31.4, 2026-09-30), Lexical (MIT, active) on ProseMirror (MIT; GitHub mirror archived) | Good (Milkdown is Markdown-native) | Web selection and scrolling, keyboard and haptic mismatch, slower start, two rendering stacks | On device (bundled JS, no network) | Yjs (MIT) or loro-prosemirror (MIT, 2026-08) | Good | Kept as **the web invite page editor** and as a spike baseline only |
| D. lexical-ios (MIT, native) | Lexical JSON, MD via plugins | Native | On device | No CRDT binding | OK | Last release 0.2.0 on 2023-11-15: too stale |
| E. Automerge (MIT, automerge-swift 0.7.2) or Yrs (MIT) as the CRDT | n/a | n/a | n/a | Both fine | Automerge slower on big text; Yjs is JS-first with weak history | Loro keeps history, checkout and forks, which Versions and Scenarios need |

How A works:
- **Model.** A Page is a Loro document: a movable tree of blocks (paragraph, heading, list
  item, checklist item, quote, code, table, divider, image/file embed, callout, live block),
  each text block a `LoroText` with marks (bold, italic, strike, code, link, mention, color).
  The editor never edits Markdown; Markdown is an import/export format.
- **Round trip.** Import with comrak (CommonMark + GFM: tables, task lists, strikethrough,
  autolinks, footnotes; front matter). Each block remembers its source slice and hash; an
  untouched block exports byte-identical, an edited one exports in a fixed house style.
  Anything we can't model (raw HTML, math, custom directives) becomes a "raw" block that is
  shown styled, preserved verbatim, and edited in place as text. Proof: a corpus journey
  (CommonMark spec examples + real READMEs) imported and exported through the CLI must be
  byte-identical when nothing was edited.
- **Typing.** Markdown shortcuts as you type (`# `, `- `, `1. `, `[] `, `> `, ``` ``` ```,
  `**x**`), a `/` menu, a selection toolbar, drag handles to move blocks, paste of Markdown
  or rich text becomes blocks. "Editar como texto" flips any block, or the whole page, to
  plain Markdown and back.
- **Rendering.** One `UITextView`/`NSTextView` on TextKit 2 per page so selection crosses
  blocks like in Notes; block styles via custom `NSTextLayoutFragment`s; non-text blocks
  (images, tables, live blocks) as `NSTextAttachmentViewProvider` views hosting SwiftUI
  (the ADR 0025 renderer).
- **Feel.** `.sensoryFeedback` on checklist toggle (success), block drop (impact), slash menu
  open (selection); a short ink flourish when a checklist completes; matched-geometry morph
  from file card to page; Reduce Motion respected.
- **E2E.** Each keystroke batch becomes a Loro update sealed into the Space log (MLS from
  M2). The relay stores and fans out ciphertext; it never sees text.

### 4. Versions and history
"Versões" shows named checkpoints and an automatic timeline (who or which agent, when, what
changed, with a visual diff), "Restaurar" creates a new version (never deletes, as today).
Page history is Loro's own; File history is the list of `MediaRef` versions.

## Consequences
- New core crate `roda-crdt` (Loro) and `roda-docs` (comrak import/export, CSV, DOCX, XLSX).
- `ItemContent::Text` stays readable for old Notes; a Note opens in the new editor and is
  migrated to a Page on first edit (expand then contract, as for every migration).
- QuickLook gives every format a viewer in the first phase; the editor list grows by phase.
- Office editing is honest about fidelity: we never silently drop content we can't edit;
  untouched parts are copied through, and full-fidelity edits go through the owner's app.

## Tests (journeys only)
- CLI journeys over the real relay: two devices import a Markdown corpus, edit, sync and
  export; the bytes match for untouched files; the relay's blob store and log hold no
  plaintext (grep for known sentences finds nothing).
- App journeys (XCUITest on one simulator): open a PDF from the Files screen and highlight it;
  type a checklist with `[] ` in a page and see it on a second device; trim a video and see
  the new version in Versões; open a .pages file and see the preview.

## At 1B users
- **Page edits.** Assume 300M people edit pages on a given day, 20 KB of compressed Loro
  updates each: 6 TB/day, about 2.2 PB after a year. Snapshots are written only when the
  update tail outgrows the last snapshot, and updates older than 90 days compact into a
  shallow snapshot plus named versions in cold storage. At $0.01/GB-month that is about $40k
  per month at year end, under $0.0001 per person per month.
- **File versions.** Chunk-level versioning stores only changed chunks. Assume 50M file
  versions a day at 2 MB new data each: 100 TB/day, half the photo volume ADR 0007 already
  budgets. Keep the last 30 versions plus named ones in hot storage and move older versions
  to cold storage ($0.004/GB-month). That adds roughly $0.3M/month at year end, in the same
  retention tiers as media.
- **Compute.** Previews, conversions and edits run on devices. The server only stores and
  fans out ciphertext, which ADR 0022 already measured.

## Build order (covers ADRs 0023, 0024 and 0025)
Each phase ships with journeys; nothing is called done on mocks.

0. **Foundations:**
   - `roda-crdt` (Loro) and `roda-docs` (comrak import/export);
   - `ItemKind::{File, Page}` with `path`;
   - files on the ADR 0007 pipeline with 4 MB chunked, resumable uploads;
   - on-device thumbnails;
   - the Markdown round-trip corpus journey.
1. **First thing people see:**
   - the native WYSIWYG page editor (headings, lists, checklists, quotes, code, links,
     images, tables read-only);
   - the text/code editor (tree-sitter);
   - the universal QuickLook viewer with markup-as-new-version;
   - import from Files/Share/drag-and-drop;
   - Versões with authors;
   - the first live-page slice (ADR 0024) on the first Zoen View slice (ADR 0025).
2. **PDF and media:**
   - PDF annotate, sign, fill forms and organize pages;
   - image edits including HEIC and RAW develop;
   - video trim/combine;
   - audio trim (extend VoiceEditor);
   - editable tables in pages;
   - agent sections on a schedule.
3. **Structured data:**
   - CSV/TSV grid editor;
   - JSON/YAML tree;
   - HTML/SVG preview;
   - ePub (Readium);
   - zip browse/extract/create;
   - usdz;
   - ontology metric/query blocks;
   - MCP Apps draft features (app tools, sampling, download).
4. **Office:**
   - xlsx in the grid with common formulas;
   - docx ↔ Page with fidelity notices;
   - "Abrir no Word/Pages/Numbers/Keynote" round trips;
   - pptx and iWork stay view-only.
5. **Together:**
   - live cursors and presence;
   - comments and suggestion mode for people;
   - forks ("Cenários") and Compare;
   - page export bundles.
