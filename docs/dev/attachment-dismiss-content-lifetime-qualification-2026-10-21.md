# Attachment content during dismissal — 21 October 2026

Native16's recorded file-menu journey at source `654d9c0c` exposed a visible
teardown error in both Preview and X close. A populated sheet replaced its file
rows and action labels with the empty illustration before leaving the screen.
The sealed independent frame review is retained at
`/Users/enzotironi/.codex/builds/zoen-native16-motion-review-v261/source-ready.json`.
The original-resolution frame pairs 588/589 and 1172/1173 show the two triggers.
They identify captured frames; they do not measure animation duration or establish
Telegram pixel or timing parity. Sparse HEVC capture produces conflicting
FFmpeg best-effort timestamps, so the review used original frame order.

`ZoenAttachmentFileContent.retire()` previously cleared its rows, selection,
labels and search text and reloaded its table before the attachment container's
exit animation. It now immediately retires action admission, extracted menus,
gestures, search focus/delegate, scrolling and accessibility while preserving
the original table cells and read-only display values. Late props and callbacks
remain fenced by the existing retired owner. No snapshot, replacement renderer,
document store or additional action authority is introduced. The removed owner
releases these original views and display values.

The host already conceals sensitive closures. That concealment now also runs
when background/context retirement overtakes an ordinary close that is waiting
for camera stop or visual removal. Camera stop still precedes container dismissal;
actual removal still precedes the closed event, parent-gallery restoration and
waiter completion. Ordinary close and keyboard return retain the original visual.

Three added UIKit regression methods cover:

- Original File and Audio cells/labels remain through retirement and a subsequent
  layout; retained controls, repeated retirement, late models and visibility
  requests cannot regain action authority.
- The search query remains visible while its delegate and focus callback retire.
- A real host with a controlled camera-stop boundary conceals an already retiring
  owner on inactivity and rejects late props/callbacks before actual removal.

These XCTest assertions are **authored and SDK-typechecked, not executed** in
this slice. The focused check uses the actual iOS SDK and production UIKit,
Lottie and Expo source/declaration closure. Only same-module test imports are
removed for that check; assertions are unchanged. The first check exposed an
explicit-self test-helper compiler error, which was fixed before the passing
second check. Both command results remain in the evidence. That compile failure
is not a functional regression RED; the observed Native16 video is the baseline
failure evidence.

Evidence: `/Users/enzotironi/.codex/builds/zoen-attachment-dismiss-lifetime-v261/`.
Source base: `654d9c0c4329b7f92937929aa9752d73efc5d41e`.
No simulator/device operation, app build, Pods, services or broad JavaScript check
ran. The Native17 integrated batch must execute the relevant UIKit tests and
repeat Preview/X dismissal, search, sensitive interruption and caption return.
The pending visual run is required before claiming that the recorded flash is
absent in the installed app.
