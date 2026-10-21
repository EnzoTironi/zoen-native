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

## Telegram iOS composer glyphs and keyboard presentation

The original attachment, microphone, keyboard, sticker and send glyphs in `apps/zoen/assets/telegram-composer/` are from Telegram-iOS contributors at revision `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`. The selected license is GPLv3 under the upstream GPL-2.0-or-later option, combined with Zoen under AGPLv3 section13. Exact upstream paths, original hashes, logical sizes and the send PDF rasterization are recorded in that directory’s `UPSTREAM.json` and `NOTICE.md`. The layout/material contract in `src/core/telegram-composer-presentation.ts` translates ChatTextInputPanelNode, ChatTextInputFontMetrics, ChatTextInputActionButtonsNode, AccessoryItemIconButton and GlassBackgroundComponent at the same pin. These assets do not include Telegram branding, messaging infrastructure or animation artwork.

The pinned Unicode emoji ordering/data and compact-grid metrics are attributed separately in `apps/zoen/modules/zoen-core/ios/EmojiKeyboard/NOTICE.md` and `provenance.json`, with the same upstream revision and GPLv3 selection. Emoji glyphs are rendered by the operating system; Telegram emoji artwork is not imported. The UIKit content adapter, variant overlay and host are Zoen implementations.

The composer field, control backgrounds and container reuse the original native
GlassBackgroundView/GlassBackgroundContainerView already inventoried under
`TelegramAttachmentPresentation`. The Expo hosts preserve the React/Fabric child
and editor ownership. Panel/clear selection also translates the original
WallpaperBackgroundNode gradient saturation calculation over Zoen's existing
SkyLayer gradient. Exact pinned sources, hashes and limitations are recorded in
`docs/product/telegram-ios-composer-glass-sources.json`; the same GPLv3/AGPLv3
selection applies. No Telegram account, network or durable state is imported.

## Telegram iOS App message presentation

`apps/zoen/src/miniapps/telegram-service-message.ts` translates the service-line normalization and legacy clear/ellipse mask from Telegram-iOS contributors at revision `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`, specifically `ChatMessageActionBubbleContentNode.swift` and `LinkHighlightingNode.swift`. The App share container in `conversation-message.tsx` also adopts the pinned `ChatMessageRichDataBubbleContentNode.swift` content/status insets. Exact source URLs, SHA256 hashes and selected line ranges are in [the source inventory](docs/product/telegram-ios-app-message-sources.json).

The upstream GPL-2.0-or-later option is retained and GPLv3 selected for this integration, combined with Zoen under AGPLv3 section13. Selected full terms are in `LICENSES/GPL-3.0.txt` and the root `LICENSE`. The mask is translated to a local SVG rendered by the existing image component; this slice includes no Telegram artwork, account services, message transport or durable state. Larger native accessibility text may expand the22pt upstream line mask to prevent clipping. Native parity remains a separate qualification.

## Telegram iOS attachment container and panel

`ZoenAttachmentContainer.swift` and `ZoenAttachmentKeyboardContent.swift` adapt the container geometry, pan/scroll arbitration, settlement, floating category layout, selection-to-caption transition, horizontal selection recognizer and selection badge geometry from Telegram-iOS contributors at revision `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`. The original files are `AttachmentContainer.swift`, `AttachmentPanel.swift`, `AttachmentController.swift`, `MediaPickerScreen.swift`, `MediaPickerGridItem.swift` and `CheckNode.swift`. Exact paths, selected line ranges, byte lengths and SHA256 values are recorded in [the attachment source inventory](docs/product/telegram-ios-attachment-sources.json).

The selected license is GPLv3 under the upstream GPL-2.0-or-later option, combined with Zoen under AGPLv3 section13. The selected terms are retained in `LICENSES/GPL-3.0.txt` and the root `LICENSE`. Zoen replaces Telegram's display/navigation framework with UIKit and the existing Expo host. Photos selection, account authorization, durable draft state and delivery remain under their existing Zoen owners; no Telegram messaging service, account data, photo library or branding is imported. This attribution does not claim complete attachment-flow or pixel parity.

The attachment settlement spring parameters and speed scaling translate `ContainedViewLayoutTransition.swift` and `CAAnimationUtils.swift` at the same revision. The entrance uses the upstream fallback without a native glass source-button frame. These animation sources and remaining adaptations are included in the same inventory.

The Photos More reference menu translates the modern `ContextControllerImpl`,
`ContextSourceContainer`, `ContextControllerExtractedPresentationNode` and
`ContextControllerActionsStackNode` plain reference/list branch at the same pin,
including geometry, immediate-touch selection, haptics and layer animations.
Exact source hashes, selected ranges and UIKit adaptations are recorded in
`docs/product/telegram-ios-reference-menu-sources.json`. The existing original
glass implementation and selected GPLv3 terms are reused. This slice does not
import Telegram's account model, extracted message rows or optional liquid-morph
runtime; existing Zoen revision/selection/caption/send authority remains intact.

The shared native gallery, selected-media mosaic, photo strip, gallery spring
and image-edit controls also translate the pinned `TGMediaPickerGalleryInterfaceView`,
`TGModernGalleryController`, `JNWSpringAnimation`, `MosaicLayout`,
`MediaPickerPhotoToolbarView` and `TGPhotoCropAreaView` implementations. Exact
sources and adaptations are recorded in `docs/product/telegram-ios-gallery-sources.json`.
The original glyphs and animation assets, GlassBackground/LiquidLens components
and supporting rendering code are attributed per file in
`apps/zoen/modules/zoen-core/ios/TelegramAttachmentPresentation/NOTICE.md`,
`UPSTREAM_SOURCES.json` and `UPSTREAM_ASSETS.json`. Original assets retain their
exact pinned bytes. Official Lottie iOS 4.6.1 (Apache-2.0) renders the original
category/Files animations; its license is included in the resource bundle.
UIKit, ImageIO, AVFoundation and PencilKit adapt rendering to the existing Expo
module. This does not claim that Telegram's entire paint/filter/video engine,
server-backed file index or messaging services have been imported.

## Telegram iOS native video scrubber and editor controls

`apps/zoen/modules/zoen-core/ios/TelegramVideoEditing/` adapts eight original
scrubber, trim, thumbnail and header sources from Telegram-iOS contributors at
revision `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`. Exact official source URLs,
SHA-256 values and deterministic scoped transformations are recorded in
`upstream-sources.json`; original Play/Gif/Mute asset provenance is in
`asset-sources.json`. Selected upstream terms are GPLv3 under the GPL-2.0-or-later
option, combined with Zoen under AGPLv3 section13. Full GPLv3 text and attribution
are bundled with the resources. The same directory's `NOTICE.md` records the
scoped display-link/namespace adaptations and the public AVFoundation player
and renderer replacement; Telegram's messaging services and converter policy
are not imported. This integration does not claim the whole video/filter/paint
engine or delivered GIF metadata parity.

## Telegram iOS File and Audio attachment list

`apps/zoen/modules/zoen-core/ios/TelegramFileAttachment/` adapts original
`AttachmentFileController`, `AttachmentFileSearchItem`, `AttachmentFileEmptyItem`,
`ListMessageFileItemNode`, `ItemListPeerActionItem`, `ItemListSectionHeaderItem`
and `ItemListSelectableControlNode` geometry/conditions and the original Search
PDF from Telegram-iOS contributors at revision
`f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`. Exact source URLs, SHA256 values,
ranges and adaptations are retained in that folder's source/asset manifests.
The upstream GPL-2.0-or-later option is retained and GPLv3 selected, combined
with Zoen under AGPLv3 section 13. Full selected terms are bundled at
`TelegramFileAttachment/Resources/Notices/Telegram-GPL-3.0.txt` and retained in
`LICENSES/GPL-3.0.txt` and root `LICENSE`. The authorized query, file transport,
preview and encrypted draft authority belong to Zoen. No Telegram account,
Postbox store, profile music or remote music provider is imported. This is a
bounded UIKit adaptation, not a full controller or runtime pixel-parity claim.
