# PdfCraft integration — preparation

Tracks #581 under #180/#170. The current source is fixed in
[upstream.json](upstream.json): PdfCraft 0.13.0,
`d445410807e7eefbe5b96f9662ab963ae8dd2a3a`. This differs from the older
2026-10-20 survey pin. No App is registered or available yet; no runtime or
performance acceptance is claimed by this preparation.

The source contains a real eframe/WebGL2 application, `PdfCraftApp`, a document
`Session`, bounded low-level COS parsing, incremental/full save, renderer and
Undo/Redo. The PDF object graph is the format model; unknown data should be
preserved rather than flattened into a custom Zoen representation. The adapter
must reuse these implementations, not replace the parser or renderer.

The standalone Web app has a same-origin parent-message bridge. It is not the
Zoen MCP App contract: its save callback considers a successfully posted message
saved, while Zoen must wait for durable Core acceptance. Its storage, URL loader,
downloads, accounts and unconstrained automation are not imported into the host.

Consumer contracts requiring actual runtime proof before profile selection:

- `Session::mark_saved` keeps Undo but replaces the current editable COS model.
  An asynchronous ACK must preserve edits made after the accepted snapshot.
- Document JavaScript is enabled by default. The managed Session must disable it
  before opening and keep it disabled; raw PDF actions do not grant capabilities.
- `save_bytes` appends incremental updates and can preserve original bytes when
  unchanged. Reconstructed files, XFA and full-rewrite operations have different
  behavior; repairs and representation loss must not be silently accepted.
- Time and authorship can affect PDF data. Do not rerun a committed command in
  another runtime expecting identical bytes; persist/replay the accepted result.
- The COS crate exposes a tighter structural inspection stream bound without a
  renderer. Its actual bounds and graph behavior must be qualified before adding
  any Core dependency or choosing document limits.
- The source already vendors renderer/platform patches. Browser preparation must
  preserve those exact inputs while adding the common managed input foundation.

Source is MIT OR Apache-2.0. Fonts, icons, CMaps, substitute fonts and other data
have separate notices in NOTICE/ATTRIBUTION. ArtCraft trademark assets have an
explicit fork-removal condition in `docs/brand/LICENSE-brand.txt`; the managed
package must exclude those marks and retain plain-text source attribution. This
is a license requirement verified from source, not a speculative rebrand.

Next: execute the original engine and editor on contributor-authored realistic
PDFs, then choose and measure a lossless profile. Integrate the shared MCP/binary
host, scoped Core ownership/CAS/receipts, full-screen touch/orientation, human and
reviewed agent edits, save/ACK/Undo recovery and selected-version export. Browser,
WASIp2, canonical and native iPhone evidence remain separate acceptance steps.
