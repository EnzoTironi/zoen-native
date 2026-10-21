# Telegram attachment presentation port

The source files identified in `UPSTREAM_SOURCES.json` and the original image/
animation assets identified in `UPSTREAM_ASSETS.json` derive from
[TelegramMessenger/Telegram-iOS](https://github.com/TelegramMessenger/Telegram-iOS)
at revision `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`.

Copyright Telegram-iOS contributors. Telegram distributes the iOS app under GNU
GPL version 2 or later, as stated on its
[official applications page](https://telegram.org/apps). Zoen selects GNU GPL
version 3 for this integration. The license text is included in
`Resources/Notices/Telegram-GPL-3.0.txt` and the repository's
`LICENSES/GPL-3.0.txt`. See also the repository `THIRD_PARTY_NOTICES.md`.

This port preserves the original UIView glass/background, container, selected
LiquidLens, touch-highlight, mesh generation, spring/layer animation algorithms
and original native availability/class/selector guards. It excludes unrelated
Telegram account, message transport, app navigation, keyboard/window access,
and ComponentFlow component wrappers. The local Expo adapter supplies theme
values and loads the bundled glyphs; it does not own identity or durable state.
The legacy image context uses UIKit's image renderer instead of AsyncDisplayKit.
Exact modifications are recorded in the source manifest. Unsupported native
lens classes follow the original guarded fallback. Runtime rendering parity
requires device validation and is not implied by copying source.

The six original attachment category animations are bundled without structural
edits. TGS payloads are decompressed to JSON without changing animation values.
Their `.still(.start)` idle / `.once` selection contract comes from the pinned
`AnimatedStickerComponent.swift`. Rendering uses the separately licensed official
Airbnb Lottie iOS renderer **4.6.1**, commit
`f4db77d7feacba0c2360b84a40c38a6ce8ff399d`, licensed under Apache License 2.0;
its original license is included in `Resources/Notices/Lottie-Apache-2.0.txt`.
The native dynamic color adapter applies the supplied tint using Lottie's
`ColorValueProvider`; accessibility reduced motion uses the original start frame.

Telegram names and artwork attribution identify the upstream source. Zoen's
product, account data, authorization and transport remain its own responsibilities.

The Files child intro uses the original `Telegram/Telegram-iOS/Resources/Files.tgs`
loop from `AttachmentFileEmptyItem.swift`, with native visibility/window/retirement
pause and reduced-motion start frame. Cloud/Image browse and crop/rotate/done/crop
corner glyphs are the original assets recorded in the asset manifest.

The Photos More menu uses the original `Chat/Context Menu/File` PDF unchanged
for its explicit Send as File(s) action. The selected terminal action contract
comes from `MediaPickerScreen.swift:3174–3181` at the same pin. Exact source
hashes and the bounded journal/Expo adaptations are recorded in
`docs/product/telegram-ios-photo-send-as-file-sources.json` at the repository root.

The composer also uses these original GlassBackgroundView and
GlassBackgroundContainerView implementations through named Expo/Fabric hosts.
The hosts retain the original React children and their frames, including the
live editor, gestures and accessibility owner. The facade accepts the original
panel/clear tint kind with a panel default for existing attachment callers.
Composer preference translates the pinned WallpaperBackgroundNode gradient
saturation calculation over Zoen's current SkyLayer colors; photographic
composite pixel sampling is not included. Exact composer/source inventory and
adaptations are recorded in `docs/product/telegram-ios-composer-glass-sources.json`
at the repository root. Full attachment source-button morph, Send blur and
glyph tint-mask integration remain separate; source reuse does not establish
runtime visual parity.

The Photos Send Without Grouping action uses the unchanged original
`Media Grid/GroupingOff` PDF. Its source pin and byte/Git-blob hashes are
recorded in the asset manifest and `telegram-ios-media-albums-sources.json`.
The conversation uses the same already attributed original MosaicLayout
implementation as selected previews; the bounded Expo geometry bridge and
canonical signed-message composition are Zoen adapters.
