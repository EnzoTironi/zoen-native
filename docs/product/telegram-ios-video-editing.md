# Telegram video editing through the existing attachment owner

This slice ports the pinned original Telegram-iOS scrubber, trim handles,
thumbnail zoom, header and their interactions into the existing iOS attachment
editor. Upstream revision is `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`.
The eight original source files, exact SHA-256 values, reproducible scoped
transformations and original Play/Gif/Mute assets are recorded in
`apps/zoen/modules/zoen-core/ios/TelegramVideoEditing/`.

Photos video Preview → Edit stages the exact ordered selection and final
per-item captions through the existing durable attachment draft. It does not
send. The approved draft then opens `ZoenAttachmentVideoEditor` instead of
Quick Look for authorized MP4/MOV/M4V editing. The native editor checks that the
actual asset is playable, exportable, finite, dimensioned and has exactly one
video track before presenting the controls. Unsupported media fails closed;
ordinary read-only gallery playback remains unchanged. This is a native format
candidate gate plus real asset admission, not an announcement of every codec,
protected stream, multi-video-track container or Photos subtype being supported.

The original scrubber controls retain the upstream 1.5-second trim-availability
gate and minimum one-second selection, hit regions, deferred thumbnail zoom,
trim delegate callbacks and animation timings. AVPlayer renders the authorized
input. The shared original glass and glyph factory supplies Cancel/Save with
PhotoToolbar's 44-point controls, 26-point portrait side insets and original
landscape side placement. No new design system, remote account or editor
storage is introduced. A scoped CADisplayLink replaces the one linear POP
frame animation; the original rest of the UIKit control code remains pinned.

## One capture and save owner

`ZoenFilePreview.Editing.receiveVideo(render:)` gives the renderer an absent
`edited.mp4` destination inside the existing protected `ZoenMedia/<UUID>`
owner directory. That lease is registered on the current FilePreview Session
before any suspension or export. AVFoundation creates its partial file under
that 0700 protected parent; the canonical owner seals the completed output to
0600 and validates actual duration, video track, MIME and byte limit before
capture. It never routes the rendered output through `receive(URL)` or creates
a second plaintext export/capture copy.

The Session owns and cancels the rendering task. Dismissal, background/account
retirement and clear cannot delete its input or resolve their continuations
until that task has drained. Failed partial-output removal stays tracked on
the same Session and then its existing retained-copy cleanup owner. A retained
copy blocks a new preview until cleanup succeeds. Cancellation cannot publish
queued or late render results.

The same actor-side draft binding revalidates engine/generation, account and
scope, native edit gate, exact revision, selected asset/mutation IDs and source
hash before rendering, after export and after actual media validation. The
existing `saveDraftMediaEdit` callback then performs its established canonical
CAS and lost-ACK retry. Its exact fingerprint, caption/order preservation,
recovery and deletion semantics are unchanged. A rejected Save retries the
same already-owned rendered file, not a fresh render with different bytes.

The renderer uses AVFoundation highest-quality output without a hidden
quality/bitrate fallback. Trimming selects actual source frames; mute removes
real audio tracks. Oversize, unsupported media and export errors are explicit.
The renderer itself does not delete a caller's output or create directories.

## Current bounds

- Edited bytes, item captions and order are saved by the existing draft owner.
  This slice does not persist a non-destructive trim recipe or original video
  source for reopening old handles. Reopening edits the durable resulting clip.
- The original `sendAsGif` affordance mutes and loops the editor preview and
  removes audio from the exported MP4. Delivery remains the existing ordinary
  video message contract. Delivered GIF/animation metadata and looping across
  clients are **not** implemented or claimed here.
- Quality/HD choices, cover editing, filters, text, stickers and video drawing
  are not enabled. The full Telegram GPU/converter engine is not imported.
- There is no new AVAudioSession override. Existing app audio policy remains
  in effect; call/recording interruption and audio focus require runtime QA.
- Main CocoaPods/Expo/FFI linkage, iPhone gestures, scrubber long press/zoom,
  rotation, memory use, software keyboard, iCloud Photos, lifecycle during
  real export and restored edited-draft usability require the integrated native
  test batch. SDK acceptance is not an iPhone interaction or pixel-parity proof.

## Reproducible focused checks

Run from `apps/zoen/modules/zoen-core/ios/TelegramVideoEditing`, with
`DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer` and fresh absolute
output directories:

- `python3 tools/vendor.py --verify`: download and verify only the eight exact
  official pinned upstream files and their deterministic transformations.
- `python3 tools/sdk-proof.py <output>`: actual scoped Objective-C protocols
  and Swift video code, real installed iPhone Simulator SDK, no declarations.
- `python3 tools/integration-sdk-proof.py <output> <built-official-Lottie-dir>`:
  actual video presentation/FilePreview/capture plus original glass, lens,
  runtime and built official Lottie closure. CoreModule receives parser-only
  validation here; main linkage belongs to the integrated app build. Existing
  upstream nullability/deprecation warnings are retained in the logs.
- `python3 tools/renderer-proof.py <output>`: actual host AVFoundation trim,
  audio removal, offset identified by independently decoded pixels, explicit
  limits/failure ownership. This is not an iOS UI proof.
- `python3 tools/capture-proof.py <output>`: seven real owner/render scenarios
  and unchanged MediaEdit(7), FilePreview(11), PDFEdit(21) host suites: 46 total.
  The presentation and save callback are synthetic; this is not Rust CAS or
  UIKit runtime proof. Partial cancellation, denied cleanup/retry, revoked
  authorization checkpoints, same-file save/reopen, rejected Save recovery,
  overlapping/stale callbacks and malformed/oversize output are asserted.

Focused Photos/native-approval JS tests execute the actual production projection
through their existing module/React test harness, including exact video Edit
handoff without Send and unsupported-format rejection. The app TypeScript
check and scoped lint are separate from native proof. Full native qualification
must use the exact final committed sources.
