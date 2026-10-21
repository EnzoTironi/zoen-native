# Telegram File and Audio attachments

The first integration exposes genuine recently sent files and ordinary audio
inside the existing Expo attachment presenter. It retains one Photos/Files/Audio
request and the existing encrypted media draft owner. The native list is a UIKit
adaptation of the pinned Telegram components; it does not introduce another
picker modal, file store or sending path.

## Upstream contract

Pin: `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`.
Exact official paths, hashes and extracted ranges are in
`apps/zoen/modules/zoen-core/ios/TelegramFileAttachment/UPSTREAM_SOURCES.json`.

- `AttachmentFileController` uses self-sent `.file` results for Files and
  everywhere `.music` results for Audio. The source API tags distinguish music
  from voice. Zoen uses its own signed presentation/MIME/voice metadata; it
  never rewrites the source MIME to qualify a row.
- File rows are real confirmed source messages with live files, current membership,
  current exact version and hash. File mode includes self-sent generic documents
  and images/videos explicitly sent as files. Audio includes ordinary nonvoice
  audio from authorized conversations, including other authors. Deleted,
  recovery-only, failed and unsent messages are excluded.
- Source row taps send unless selection is active; then they toggle selection.
  Search rows have no multi-selection. The search button follows the source's
  initial loading or more-than-ten-results condition; search is unavailable
  during selection. File has Gallery and Files actions; Audio has Files.
- Original document vectors/gradient colors, 40pt icon, 16/14pt title/subtitle,
  45pt selection offset, 26pt selection control, generic action layout,
  section-header geometry, empty intro layout, Search PDF and 40pt close path
  are retained. Theme values come from Zoen's current presentation palette.

## Selection and ownership

The disposable query projection uses a bounded tuple-keyset/FTS index. Pages
contain at most 50 rows; the picker retains at most 500 and asks the user to
refine the search rather than scanning unbounded history. The selection is at
most ten files and 64MiB total. Signed source references are copied before being
retained; borrowed query objects cannot mutate an accepted selection.

A recent row is materialized through the existing exact-version demand and
private URI owner. Account, destination and source are checked again after the
await. The native actor binds that URI to the source reference, bounded to 20
live copies. Save passes these bindings to the canonical encrypted journal:
source authorization/current version/hash and actual copied bytes/name/MIME are
validated atomically with the destination CAS. The destination receives new
asset/file identities and keys. No source keys or source ID is sent as a new
attachment. Once saved, the draft's own identities are authoritative.

Lost-ACK retries preserve the source-aware fingerprint until the URI is released;
an exact receipt replay remains valid after the source is deleted or its role
changes. Fresh writes still revalidate the live source. Native release and SDK
cache cleanup run through the existing preview/cache owners.

The final native caption is committed only after materialization succeeds.
A failed recent Send cannot make a later Files selection send accidentally.
Browse/category replacement is blocked while recent selections remain active.
Preview waits for the source panel's real removal, opens a read-only canonical
file/player preview, then resumes the same child. It carries an AbortSignal,
UUID and the existing native presentation task/lifetime gate; route retirement,
background, account closure and erasure cannot present a stale preview.
Audio Browse uses Expo SDK58's `audio/*` document filter and never starts a
voice recorder or creates voice metadata.

## Qualified scope and remaining work

The TypeScript integration and focused production-session tests cover exact
selection order/final caption, stale render/owner/query completions, aggregate
limits, partial-copy cleanup, immutable source references, failed Send followed
by Browse, preview/removal/resume and cancellation while the same Photos token
is draining. Rust tests cover authorization, versions, fresh destination
identities, receipt replay and bounded SQL selection. SDK checks compile the
actual UIKit list and original glass/Lottie dependency closure. They do not
prove Fabric mounting, generated FFI linkage or device rendering.

The original Postbox/ListView controller is not imported wholesale. This slice
has no Telegram online music bot, profile music, scanner, thumbnails/album art,
performer/duration extraction, inline music playback, file download-progress
row animation, custom source context-menu animation, full no-results query
subtitle or Dynamic Type parity. Music's play control opens the existing player
preview and resumes the list; it does not pretend to play inline. Loading uses
a real spinner rather than eleven fake message rows. The query runs locally
without the source's remote-search delay. Unsupported categories stay absent.

A rebuilt Rust/UniFFI framework, pod resource registration, main iOS build and
on-device validation remain required before claiming this integration shipped.
Web and Android do not expose the native recent picker until their equivalent
owners and presentation are qualified. No network music provider or filesystem
is enumerated automatically.
