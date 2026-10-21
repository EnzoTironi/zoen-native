The native video controls and canonical capture bridge are integrated into
ZoenCore in this isolated batch. Main app/device qualification remains pending.
See docs/product/telegram-ios-video-editing.md for actual behavior and bounds.

Native build integration

1. Add `TelegramVideoEditing/Sources/*.h` to ZoenCore.podspec public headers so
   Swift imports the actual scoped Objective-C controls and UI-only protocols.
2. Add resource bundle `ZoenTelegramVideoEditing` with
   `TelegramVideoEditing/Resources/**/*`. No extra pod is required.
3. Existing source_files includes these Swift/Objective-C files. Keep UIKit,
   AVFoundation, CoreMedia and QuartzCore linked; regenerate Pods normally.
4. Use the existing shared Telegram toolbar factory for the controller's two
   UIControls. The root gallery may instead embed content/header directly.

API

```swift
let glyphs = try ZoenTelegramVideoGlyphs.bundled()
let content = try await ZoenTelegramVideoEditingContent.prepare(
  asset: authorizedAsset, initialState: restoredDisplayState,
  allowsTrimming: authorizedTrimCapability, glyphs: glyphs,
  fileInfo: actualSourceFileInfo)
// content.view: video/player/play/mute, content.headerView: original timeline
// setVisible(false) suspends decode, zoom callbacks and playback nonterminally.
// retire(sensitive:true) permanently fences and conceals this presentation.
// willPlay/didPause join the host's existing audio-focus coordinator.
```

The host must supply actual PHAsset/asFile/high-framerate capability rather than
guessing it from an AVAsset URL. The component applies the source1.5-second trim
availability gate; the original scrubber enforces a1-second smallest selection.
The source default is a nonmuted video. sendAsGif mutes actual playback, loops
the selected range, and removes exported audio. No delivery message is sent by
this component. UI changed callbacks are never durable acknowledgments.

The optional `ZoenTelegramVideoEditingController` receives content, original
Cancel/Commit UIControls and:

```swift
save: @MainActor (AVAsset, CMTimeRange, Bool) async throws -> Void
cancelled: () -> Void
saved: () -> Void
failed: (Error) -> Void
```

`saved` requests host continuation only after that Save closure succeeds.
FilePreview remains the owner of actual dismissal, failed capture, retry,
accepted edit, recovery and cleanup. On native close/erase/background cancel
the existing edit gate first, then retire/cancel the controller. Completing a
stale Save must not reopen or mutate a replaced gallery owner.

Canonical export bridge in the existing owner

* The async Editing.receiveVideo(render:) operation is backed by
  existing ZoenMediaEditCapture. Allocate/register one private ZoenMedia UUID
  directory with0700 in that owner BEFORE render; seal the actual output0600 before ingestion.
* Call `ZoenTelegramVideoRenderer.render(asset:range:sendAsGif:destination:
  maximumBytes:progress:)` using that canonical destination. Do not route it
  through receive(URL), which would introduce a second plaintext copy.
* During export that canonical owner tracks the output, cancellation and every
  partial file. On failure remove it through existing tracked cleanup; retain
  it and report cleanup failure if deletion fails. The renderer never deletes
  an unknown destination or creates its own temporary namespace.
* On success validate real video tracks, finite duration, MIME, size and hash,
  then pass the same ZoenPreviewFile into existing captured callback/CAS.
* Revalidate account/device, scope, exact selected mutation/asset/hash and base
  revision after every await. Preserve current lost-ACK/recovery behavior.
* Persisted trim/mute recipes and delivery animation/GIF metadata are not part
  of this byte-edit slice. Reopening edits the saved MP4; preview loop does not
  imply delivered-message looping. Do not create a second persistent UI store.

`ZoenTelegramVideoRenderer` uses native highest-quality encoding with no hidden
lower-quality or bitrate fallback. It checks the existing byte limit during
export and after completion. Oversize is an explicit failure; the caller owns
the partial output. It preserves source display transform and real selected
frames; it does not copy the unrelated legacy converter's0.1-second offset.

Qualification

`tools/renderer-proof.py <new output directory>` compiles the real stateless
renderer for macOS, produces a synthetic video/audio fixture, executes actual
exports and verifies offset via independent decoded pixels. This is not an
iOS UI, Rust CAS or lifecycle proof. Integrated native QA must still cover
gesture arbitration, timeline long press/zoom, swipe interruption, mute/loop,
rotation, background, source revocation, Save rejection and canonical recovery.

The focused UIKit/Objective-C and Swift SDK check is reproducible with:

```sh
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  python3 tools/sdk-proof.py /absolute/new/proof-directory
```

Run from this directory. It imports the actual original ObjC protocol headers,
uses the installed iPhoneSimulator SDK and supplies no declaration/render stubs.
