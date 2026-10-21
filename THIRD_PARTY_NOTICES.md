# Third-party notices

This file adds integration-specific attribution. Existing upstream license and
notice files, including those under `miniapps/` and the Expo native module,
remain applicable. It is not a complete dependency inventory or release audit.

## Telegram iOS check geometry

`apps/zoen/modules/zoen-core/ios/ZoenTelegramCheckGeometry.swift` translates the
fractional procedural glyph data in `generateCheckImage(partial:color:width:)`
from TelegramMessenger/Telegram-iOS contributors. It is derived upstream data,
not independently created artwork.

- Repository: <https://github.com/TelegramMessenger/Telegram-iOS>
- Revision: `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`.
- File: `submodules/TelegramPresentationData/Sources/PresentationThemeEssentialGraphics.swift`, lines 8–24.
- Upstream file SHA256: `d9373e9220ce3c245b9b34ad296677556eb76505bedfa76a4929f74da3769ed2`.
- License: GPL version 2 or later (`GPL-2.0-or-later`); the GPLv3 option is selected
  for this integration. Full selected terms: [GPL-3.0](LICENSES/GPL-3.0.txt).
- Changes: the SVG path operations are represented as typed CoreGraphics segments
  for Zoen's native receipt renderer. Telegram account services and messaging
  infrastructure are not included by this reuse.

The pinned source has no individual copyright header; attribution is to its
upstream contributors. Telegram's official source-code distribution identifies
the iOS client as GPL version 2 or later:
<https://telegram.org/apps/download?setln=en>.

## Telegram presentation and static emoji rules

`apps/zoen/src/theme/telegram.ts` translates presentation values and the Classic
accent-to-outgoing-color rule from the pinned Day/Night factories. The static
emoji size table in `apps/zoen/src/core/message-presentation.ts` translates
`fontSizeForEmojiString` from `ChatMessageAnimatedStickerItemNode.swift`.
The read source revisions and SHA256 values, including the referenced chat-list
and customization files, are in
[the pinned source inventory](docs/product/telegram-ios-design-system-sources.json).

- Upstream contributors: TelegramMessenger/Telegram-iOS contributors.
- Revision: `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`.
- License: `GPL-2.0-or-later`; GPLv3 is selected for this integration, as above.
- Changes: Swift/UIKit presentation values are expressed as typed TypeScript
  roles, bounded local preferences, two Zoen sky families, and native color
  controls. Static emoji retain Zoen's real text and receipt authority; no
  Telegram emoji/sound/font asset or messaging service is included.
- Inspection of the Tinted factory does not add a shipped Tinted preset.

The new sky contrast constraints, preference persistence, native gradient mask,
and React Native screen adapters are Zoen implementations. This attribution
is not a claim of complete Telegram behavior or pixel equivalence.

## Telegram iOS composer and voice presentation

`apps/zoen/src/core/telegram-audio-presentation.ts` translates composer font metrics, reply/edit accessory dimensions, audio waveform binning/cap dimensions, microphone gesture thresholds and transition durations from Telegram-iOS contributors, revision `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`. The exact upstream paths, SHA256 and URLs are in `docs/product/telegram-ios-composer-audio-sources.json`. Source files include `ChatTextInputFontMetrics.swift`, `ReplyAccessoryPanelNode.swift`, `EditAccessoryPanelNode.swift`, `AudioWaveformNode.swift`, `ChatRecordingPreviewInputPanelNode.swift`, `ChatMessageInteractiveFileNode.swift`, `ChatTextInputMediaRecordingButton.swift`, `TGModernConversationInputMicButton.m` and `CAAnimationUtils.swift`.

The source is GPL version2 or later; the GPLv3 option is selected for this integration, with full selected terms in `LICENSES/GPL-3.0.txt`. Upstream files have no individual copyright header; attribution is to their contributors. Zoen adapts the contracts to React Native and the existing authenticated capture/playback/durable-draft owners; Telegram account services and media transport are not imported. Recording release remains explicit review→Send, and the detailed transcript/filler editor is Zoen functionality. No upstream raster/vector artwork is included by this slice.

The profile geometry contracts in apps/zoen/src/profile/telegram-profile-presentation.ts and their RN presentation use source-observed/translated Telegram-iOS PeerInfo geometry from Telegram Messenger contributors, revision f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838. The upstream GPL-2.0-or-later option is retained, with the GPLv3 option selected for combination with Zoen's AGPLv3 project under section13. See the existing root GPLv3/AGPLv3 license texts. Exact primary source paths, URLs, hashes and lengths are in docs/product/telegram-ios-profile-sources.json. This is not an upstream account/storage/protocol import. No new raster/vector artwork or Swift implementation is embedded by this packet; the translated contract is still attributed as derived material, not claimed to be clean-room.
