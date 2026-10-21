This directory adapts selected code and original assets from Telegram-iOS,
revision `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`, published by
Telegram Messenger: https://github.com/TelegramMessenger/Telegram-iOS.

Telegram's official applications page states GPL version 2 or later:
https://telegram.org/apps. Zoen selects GPL version 3 for this integration.
The GPL-3.0 text from this repository's existing `LICENSES/GPL-3.0.txt` is
included under `Resources/Notices/GPL-3.0.txt`. See the repository license and
third-party notices for the combined AGPLv3 work.

`upstream-sources.json` records the exact upstream bytes and the resulting
namespaced control hashes. `tools/vendor.py --verify` downloads only those eight
pinned files and verifies the reproducible transformation. `asset-sources.json`
records the original Play/Gif/Mute vector/bitmap bytes. No third-party font,
account, remote sticker provider or network protocol is introduced.

The eight scrubber/trim/thumbnail/header sources preserve the original gestures,
trim and zoom algorithms, shapes, dimensions and animation timing. Adaptations:

- Classes are prefixed with Zoen. Module umbrella imports are replaced with
  scoped UIKit color/font/rotation helpers.
- UIKit control subclasses implement the same expanded hit rectangles instead
  of swizzling every UIControl in the process.
- Display scale comes from the actual view's traits. The original separator
  policy (0.5 at 2x, 0.33 at 3x) and thumbnail stripe policy are retained.
- The one POP linear frame animation is driven by a scoped CADisplayLink with
  the same linear interpolation, clamping and half-point rounding. POP itself
  is not bundled; all other original UIKit animation calls remain upstream.
- The UI-only delegate/data-source protocols are annotated MainActor for Swift.
- An explicit suspension hook cancels deferred zoom and display animations.

`ZoenTelegramVideoEditingContent` adapts the original video item callbacks using
AVPlayer and AVAssetImageGenerator. `ZoenTelegramVideoRenderer` performs actual
trim and audio-track removal through AVFoundation into a destination allocated
by the existing canonical capture owner. It intentionally does not copy the
Telegram converter's encoding presets or its 0.1-second trim offset. It creates
no temporary directory, never overwrites existing output, and never deletes a
caller's partial output. The canonical owner remains responsible for cleanup,
validation, authorization, CAS, retry and recovery.

The optional controller requires the host's shared original Cancel/Commit
controls. It does not implement a competing glass/pressed-state design system.
`ZoenTelegramVideoEditorLayout` translates the pinned item/header setup and
gallery final header/safe-area geometry. The initial44-point header seeds a
scrubber origin of0; final headers are72 points in portrait and64 in landscape.
The original64-point panel and68-point scrubber remain unchanged. Landscape
headers exclude the side toolbar and consume its safe inset exactly once.
The standalone editor retains its top safe-area anchor rather than importing
the gallery caption/navigation authority. Sources and exact hashes are recorded
in `docs/product/telegram-ios-video-header-sources.json` at the repository root.
Its horizontal footer uses MediaPickerPhotoToolbarView's44-point controls and
26-point side inset. Gallery hosts can embed the same content and retain their
existing caption, selection and footer. Landscape host layout, source image
transitions, video-cover editing, quality presets, filters, drawing, text and
stickers require subsequent integration; this subset does not claim their UI.

Host renderer proofs exercise real macOS AVFoundation and independent ffmpeg
decoding, not an iOS UI renderer or a stand-in SDK. SDK typechecking is separate
from simulator/device playback, keyboard, interaction, memory and recovery QA.
