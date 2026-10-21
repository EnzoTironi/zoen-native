# File row extracted menu

This bounded port replaces the normal recent File row's UIKit context menu with
the actual-content `.extracted` contract from Telegram iOS
`f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`. Photos continues to use its distinct
plain `.reference` source. Audio and search keep their existing UIKit menu.
Zoen's existing Preview and Select actions, authorized source IDs and canonical
draft remain unchanged. Opening the menu does not send or select a file.

## Original source

`AttachmentFileController:706–738,1118–1137` supplies an extracted source with
`keepInPlace=false`, `ignoreContentTouches=true` and `blurBackground=true`.
`ContextControllerExtractedPresentationNode:255–300` takes the actual content
view; it does not make an image of the row. The port moves a dedicated native
row child containing the existing icon, labels and button. UIKit's cell
`contentView`, selection control and separator stay in the table.

`ListMessageFileItemNode:494–541` places the extracted background and foreground
in that child. The modal background uses the original day white/night #2c2c2e,
28pt stretchable-circle diameter and 12pt foreground offset. Normal recent File
rows have no global-search date to fade. The existing table is already inset
16pt at each edge, so the adapter takes its owned row bounds as the normalized
content rectangle; this does not port the whole upstream list/header hierarchy.

The original geometry uses status-bar height plus 8pt above, 10pt plus intrinsic
bottom inset below, a 7pt content/action gap, 12pt menu edge bounds and the
source-dependent left/right action alignment. The content and actions travel
from their original screen position using the existing original Display spring:
0.42s, damping 104 and velocity 0. The action opacity lasts 0.05s; background
appearance lasts 0.2s. The row's extracted style uses the original 0.5s spring.
Dismissal samples in-flight presentation values and returns the actual row
using 0.2s easeInOut before removing its presentation owner.

The backdrop translates `NavigationBackgroundView:62–139`: the original `.light`
blur, filtered blur/saturation layers, original context dim colors and opaque
Reduce Transparency fallback. The transparent outside-dismiss layer retains the
already integrated iOS 27 touch-delivery workaround. Photos gains no blur or dim.

`ContextGesture:74–253` supplies the 0.12s delay and 0.2s linear activation driven
by the existing original `SharedDisplayLinkDriver`. Source compression follows
`ContextControllerSourceNode:97–128`, including the 15pt/0.7 minimum and 0.2s
easeOut return. UIKit table deselection replaces Texture selection cancellation.
The retained source gesture can drag into the action list after the original
4pt vertical threshold. Its first external move establishes the baseline; the
activation point is not used. `ContextControllerImpl:494–500` marks the modern
source-container branch ready synchronously before starting its animation. It
does not wait for the 0.42s spring; the legacy blur-readiness path is different.
Enabled actions retain the existing selection haptics,
highlight geometry and once-only dismissal/action gate. Canceling never performs
an action. VoiceOver receives the localized More action and Escape/outside close.

The original Select PDF is included byte-for-byte, with its hash in the existing
presentation asset inventory. Preview is an existing Zoen extra without invented
artwork; no new file or caption authority is introduced.

## Ownership and bounded adaptations

The common renderer has separate reference and extracted sources. The extracted
lease preserves the actual content subtree, original interaction flag and source
ordering. It restores synchronously before any row update, reload, reuse,
visibility loss or retirement. Retained actions check the current revision,
source identity, category, visibility and busy state before the existing native
session event can run. Final caption and durable send ownership stay outside the
menu. Opening, closing or repeating a canceled action cannot publish a message.

`AttachmentFileController:724–734` starts dismissal and then updates selection
immediately; it does not put Select in the dismissal completion. Zoen preserves
that order and advances native revision, busy and selection state immediately.
Only table reconfiguration is deferred during that action's 0.2s return, so the
actual content cannot be reused before being restored. Pending renders collapse
to the latest current state. Disabled/revoked sources, category/search/theme
changes, visibility loss and retirement still interrupt synchronously. The
existing Zoen-only Preview action instead waits for normal content restoration
before handing off to a viewer; canceled/interrupted or stale returns discard
that pending handoff. Photos Send-as-File remains immediate after dismissal
begins.

Reduced Motion preserves opacity while omitting transforms. Parent/window loss
and app deactivation remove the overlay and restore its real row immediately.
Unexpected source-size changes cancel and restore into live row bounds before
UIKit reuse. The follow-up [native window policy](telegram-ios-presentation-policy.md)
ports the original current-orientation lock for both context source types and
status hiding for extracted blur, including their distinct dismissal lifetimes.
Reactions, portal surfaces, other extracted messages and Gallery Add are outside
this batch.

## Qualification

Exact upstream URLs, hashes, ranges and adaptations are in
[`telegram-ios-file-extracted-menu-sources.json`](telegram-ios-file-extracted-menu-sources.json).
Isolated evidence is in
`/Users/enzotironi/.codex/builds/zoen-extracted-menu-v253/`.

Executed: the actual production Foundation/CoreGraphics extracted geometry,
including a pinned near-bottom row anchor, 144 layouts, source visual scales and
overflow. The unchanged reference geometry/lifecycle was also executed across
144 layouts. The actual UIKit menu, File caller, Photos caller, shared owner,
original glass/Display/Lottie and compiled ExpoModulesCore/React dependency
closure are SDK-typechecked together with the authored UIKit ownership,
revision, interruption and resize tests and existing caller tests. Test copies
change only the target-module import for this standalone check.

The UIKit tests are authored and typechecked, not executed in this batch. There
are no renderer, hierarchy or Expo substitutes. No simulator, app, main build,
Pods, core build, service or LIVE source was used. The next integrated batch
must exercise physical long press and source drag, Preview/Select, outside and
Escape cancellation, interrupted opening/return, day/night blur, parent loss,
source-size change and final-caption retention. Arithmetic and typechecking do
not establish pixel or delivered-touch parity.
