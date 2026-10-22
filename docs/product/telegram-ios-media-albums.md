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

## Explicit recovery of refused media

An own Failed attachment or voice note can offer **Prepare New Send** in the
existing message menu. Native `failedMediaRecoveryVersion=1` gates this action;
older binaries, Web and Android do not advertise an unsupported operation.
The action prepares the existing private composer. Sending remains a separate
explicit action, and the original Failed message remains in history.

`recover_failed_media_draft` checks the actual signed Failed outbox event,
identity/device certificate, current writer access, exact reply/thread scope
and known local confirmed log. Album recovery requires every signed member,
ordered by its explicit index with matching count, presentation and destination;
it never groups by time, name or visual proximity. Both the normal and media
drafts must be empty at the expected revisions. These CAS reads, bounded verified
chunk reads and encrypted journal copies share one SQLite transaction. The
shared journal persistence helper owns asset encryption, quotas and replacement.
Recovery adds no schema, relay endpoint, transport queue or send-intent table.
A rejection rolls back only the private journal, without reloading/pruning the
canonical projection or outbox.

Every recovered asset and attachment mutation receives a new identity. Captions
and file/media presentation are retained. A voice note's final transmitted bytes
become a new source, decoded and authenticated by the existing native voice
preparation path. That creates a fresh edit recipe; it does not reconstruct the
original recording, removed audio or undo history. No audio can send before
preparation produces its new render.

The normal-draft admission barrier never flushes or retries an ambiguous Send.
Late results cannot replace a composer whose local revisions, session or channel
changed. Concurrent typing stays intact. After a lost recovery acknowledgement,
explicit Retry reloads the journal rather than repeating recovery or sending.
Original file-send receipts and Failed outbox entries are not rewritten. Local
confirmed history rejects recovery; unavailable remote confirmation cannot be
treated as proof of global deduplication. Local discard is outside this change.

The core media-draft batch covers recovery, rollback, signed/device and scope
negatives, complete/partial albums, late confirmation, byte corruption, cold
reopen and the final-audio source boundary. Node tests exercise actual draft
controllers, service/adapter and compiled menu callbacks, including ambiguous
normal sends, concurrent typing, retirement, lost acknowledgements and older
native capability gates. Installed iOS menu, voice preparation and recovery
journeys still require the next native runtime batch.
