# Signed media albums in the iOS conversation

A fresh Photos selection of two through ten assets now uses Telegram's grouped
default. More → Send Without Grouping uses the pinned original GroupingOff PDF
and the same terminal action, final caption, selection order and native-removal
barrier as Send as File(s). A single asset keeps the legacy individual envelope.

The existing Rust `AttachmentSendOptions`, journal/CAS, `MessageMedia.group`
metadata and atomic send path own persistence and identity. No schema, storage,
timestamp heuristic or alternative sending engine is added. An existing draft
retains its admitted presentation/grouping on append; the new picker cannot
convert a batch whose older members it is not displaying. Explicitly removing
an album down to one member saves a valid individual-media draft.

The conversation reuses `mediaAlbums` to recognize complete signed membership,
then calls the original Swift MosaicLayout algorithm through a bounded pure Expo
Function. Each tile retains its canonical message renderer, exact-version file
preview/copy lease, native hold menu, reply/thread targets and signed caption.
Aggregate receipts never claim delivery/read before every member reaches it.
Search/quote reveals temporarily retain individual canonical rows. Partial or
conflicting membership, deleted/forwarded/voice entries, and groups containing
existing per-message reactions or reply counts remain individual rows so those
affordances are never hidden. Web/Android and an older native binary also retain
individual rows until their mosaic adapter is qualified.

If canonical file metadata identifies a generic/audio member or a file above
the existing 16 MiB inline-preview limit, every member returns to its ordinary
row. A delayed callback for an older file version cannot disband its successor.

This is an iOS integration of existing contracts. It does not qualify Telegram's
entire album behavior: grouped reaction presentation, grouped document rows,
HD/HEIC conversion, spoilers and caption-above-media are outside this slice.
Initial tile geometry uses square placeholders until exact owned image/video
pixel dimensions arrive; it then uses the original algorithm. No file bytes or
new document authority cross the geometry bridge.

Source pin, original asset hashes and adaptation boundaries are recorded in
[the album source inventory](telegram-ios-media-albums-sources.json). The source
and original assets retain the existing selected GPLv3 / Zoen AGPLv3 attribution.

The Node tests exercise production Photos/draft admission, final-caption/order,
changed-revision refusal, recovery/replay, per-cell canonical callbacks and safe
projection/fallback. The geometry host proof executes the actual original Swift
algorithm and bounded adapter. UIKit XCTest additions cover the typed native
menu and simultaneous table highlight on extracted-content admission/return;
those tests still require the integrated iOS runner. Native rendering, physical
gestures, independent recipient delivery and a cold native album journey require
the next installed application batch; source or host checks alone do not imply
that acceptance.
