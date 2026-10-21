# Conversation media viewer

The iOS conversation opens the chosen canonical photo/video in a read-only pager.
A complete signed album supplies at most ten candidates. Native code queries
each exact message using the existing `message` command, checks item/version,
and reconstructs author/group/index/count/Thread/reply/presentation membership.
The request order, timestamps and filenames cannot create membership. A single
message remains a single page; this delivery does not scan or copy other history.
The existing `message` query internally projects the timeline; it is not a new
indexed message lookup or a scalability qualification of that query.

`ZoenAttachmentGalleryContent` supplies the already ported image/zoom/pan/glass
and video thumbnail controls. Conversation mode removes Send, selection, editing,
reorder and add controls, even if a model attempts to enable them. It shows the
current canonical author, caption and position. Conversation videos use one
read-only Telegram transport over AVPlayerLayer, with the Gallery's single close
control. The pinned viewer's64/92/30pt play/pause/15-second seek controls, original
play/pause animation and arrows,44pt scrubber, elapsed/remaining text and source
footer insets are adapted to UIKit using the already ported original glass and
official Lottie renderer. `TelegramConversationVideo/SOURCES.json` records the
exact upstream blobs, ranges and adaptations. No AVKit chrome competes with the
Gallery toolbar. Photos/durable attachment preview and explicit editing remain
separate existing presentations.
Documents/PDFs, including their existing explicit edit/recovery flow, remain on
Quick Look. Unsupported visual formats keep an authorized single-file fallback.

`GalleryPagerNode` supplies the 20pt edge gap and current±1 window contract.
`GalleryController` supplies chosen-message identity and per-page chrome.
`GalleryControllerNode` supplies the simple presentation fallback: backdrop and
chrome0.15s, content0.3s; close chrome0.1s/content0.25s, vertical dismiss after
one-quarter height or1000pt/s. Reduced Motion removes those animations. Native
source-to-image hero transitions are not claimed: Fabric source rows may have
virtualized. Closing restores the existing conversation/scroll origin without
retargeting a first album child or requiring the source row to stay mounted.
Neighbor arrival preserves the current page, zoom and player instance.

Playback does not require exportability or exactly one video track. The transport
uses the already authorized private URI; it never copies, exports, edits or saves
media. A paused current page prepares its player/controls; neighboring pages do
not start players. Periodic observation runs only while the page is active.
Paging pauses, deselection cancels pending seeks and observation, and retirement
rejects queued actions and releases observers. Reactivation stays paused. Relative
scrubbing commits an exact seek on release, restores the origin on cancel and
resumes only the prior active playback. The44pt adjustable scrubber exposes
15-second accessibility steps; hidden/inactive/retired controls cannot dispatch.
Caption and transport occupy distinct footer rows in portrait and landscape.
The original footer bounds long captions: above100pt of text uses an80pt
scrolling viewport; compact landscape hides the caption. A regular short
viewport additionally clamps this caption against the actual92pt centered
control and44pt footer, so readable caption content cannot intercept playback.

All temporary copies belong to `ZoenFilePreview.ReadOnlyCopies`, including the
initial page. The owner enforces three copies, retires display requests/players
before eviction and waits for actual hierarchy dismissal before final cleanup.
Late asynchronous arrivals are deleted under the same owner; deletion failures
remain in its existing retryable cleanup list. No second durable cache, journal,
document state or file-key authority is introduced. A page fixes its hash before
materialization and rechecks canonical message, account/device, native generation,
version/hash and owned-file binding after every await.
Every page transfers through the existing locked `Scope` before the actor returns
its ticket. Closing drains that viewer's pending tickets as well as visible leases;
its retired registration refuses and deletes a late producer's copy. Account clear
also drains pending generations. Tickets from another viewer cannot be adopted.
If late deletion fails after the registration closed, the next preview and every
explicit clear promote that failure into the existing retained cleanup list. The
busy guard prevents another preview until cleanup succeeds; clear also collects
failures that arrive while a native hierarchy dismissal is still pending.

The existing native `draftMediaEditLifetime`, cancellation tokens and preview task
registry also fence background/foreground, early cancellation and account cleanup.
The JS viewer lease is independent of row mounting. It revalidates on canonical
changes and aborts on deletion, changed messages or session retirement. The store
cleanup waits for the native Promise to finish. Other platforms and older native
builds retain the existing single-file preview path.
Native source changes also coalesce a bounded revalidation of the active request,
so revocation does not depend on a responsive JavaScript renderer. This is an
ephemeral request snapshot, not a second message projection or authorization store.

Expected native lifecycle cancellation completes as a normal read-only close only
after the existing FilePreview owner has dismissed its hierarchy and verified copy
cleanup. Real cleanup and presentation failures still reject. A failed viewer
action does not mark the inline file as unavailable: signed albums retain their
mosaic on background/close or a busy presenter. Actual inline metadata, copy,
image-decode and player failures retain their existing canonical-row fallback.

Qualification is scoped: Node contract/production composition tests, actual host
FilePreview cleanup tests and private UIKit SDK typechecking. Authored UIKit
regressions cover chosen viewport/gap, read-only chrome, bounded pages, neighbor
replacement and stale paging revisions. New playback-only host scenarios execute
real AVPlayer time/seek/scrub/cancellation and a two-video-track composition. New
UIKit transport regressions are authored and SDK-typechecked, **not executed**.
Simulator/render/hit-test/real keyboard,
full Expo linkage, physical devices, media performance and full Telegram Gallery
parity are separate pending gates. Advanced gallery sharing/actions, recognition,
PiP and history pagination are outside this slice; existing per-message sharing
remains available. No service or user data is changed by the proof harness.
Viewer speed, chapters, scrub-frame previews, vertical precision scaling, fetch
shimmer, interactive glass morph and blur visibility transitions are not ported
by the bounded playback controls adaptation. Source metrics/assets do not imply
full Telegram pixel or interaction parity.
