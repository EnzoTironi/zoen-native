# 0051 — Bounded recent attachment sources and authorized copies

Status: implemented for review; iOS presentation and device validation are separate.

Telegram iOS revision `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`
uses `sentMedia(tags: [.file])` for recent Files and a general, everywhere
`.music` search for Audio. The source is
`submodules/TelegramUI/Components/AttachmentFileController/Sources/AttachmentFileController.swift:742–761`.
`submodules/TelegramCore/Sources/ApiUtils/StoreMessage_Telegram.swift:47–97`
refines generic file tags using video, audio, voice, sticker and animation
attributes. Zoen does not have that complete attribute schema. Its adaptation
uses the current real MIME plus signed file presentation: ordinary generic files
and images/videos explicitly sent as files belong to Files previously sent by
the current account; ordinary nonvoice audio belongs to Audio from all authors.
Any voice version excludes the source. Library-only Items, deleted messages,
unconfirmed sends, recovery-only files and inaccessible conversations are absent.
This is an explicit schema adaptation, not a claim of identical media tagging.

`recent_attachments` takes the existing exact identity/device/destination/thread
draft scope, kind, query (at most 512 UTF-8 bytes), optional cursor and limit
1–50. Results contain metadata and an exact source reference: conversation,
message, File Item, current version and SHA-256. They contain no file URLs,
private keys or media bytes. Cursors bind the full scope, query, kind and committed
index generation; mutation invalidates pagination explicitly.

The existing FTS rebuild classifies attachments during its normal traversal.
The same savepoint rebuilds a disposable `attachment_recent` B-tree accelerator
containing FTS row identities, source entry positions and ordering metadata.
Complemented signed timestamps reverse time order exactly, allowing a single
indexed tuple seek across timestamp/conversation/message without OFFSET or
scanning earlier pages. FTS text membership is applied before LIMIT. Neither
table is an authority. Canonical entry identity, author, membership, deletion,
classification, File ownership, current version/hash and bounded file metadata
are checked against the current signed projection for every returned source.
An absent accelerator is rebuilt; a corrupt returned position causes an explicit
error and repair, rather than a successful empty page. Opening the core always
marks the complete search projection for reconstruction.

The bounded guarantee applies to the new recent-source query and source-row
resolution. The reused private-draft scope validator still calls the existing
`reply_ref` linear conversation-entry lookup when a thread root is supplied.
That pre-existing authorization cost is preserved; this batch does not claim to
make thread-root validation indexed or change its behavior.

`attachment_reference_validate` supports the existing exact-version native file
demand before and after asynchronous materialization. The additive
`save_media_draft_with_recent_sources` closes the final race: under the existing
core lock it validates every supplied source binding, then checks actual input
SHA-256, name, MIME and size before the existing encrypted journal/CAS writes.
At most ten bindings can name supplied assets; duplicate or missing bindings
are refused. Input bytes retain the existing 64 MiB attachment budget. One bad
source prevents the entire batch from staging. Derived search repair is allowed;
refusal creates no draft, event, media chunk, file key or upload.

Source-aware saves use a domain-separated fingerprint including sorted source
bindings. Exact accepted receipt replay precedes live source validation: losing
the acknowledgement does not invalidate an already-authorized private copy when
the source is later edited, deleted or access is revoked. Fresh requests require
live authorization. Existing saves with no bindings retain their exact original
fingerprints and receipt behavior. After acceptance, the canonical private draft
owns the copied bytes independently of the original source. Explicit Send uses
the existing transaction to create a fresh destination File identity and keys.
No extra journal, account import, fake MIME, quality transform or automatic Send
is introduced.
