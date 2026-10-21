# Telegram iOS Unicode emoji catalog

The emoji order, eight category boundaries and skin-tone group data in
`ZoenEmojiKeyboardData.swift` are derived from `emoji1.txt` in
TelegramMessenger/Telegram-iOS, revision
`f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`.

- Upstream contributors: TelegramMessenger/Telegram-iOS contributors.
- Pinned resource: `submodules/TelegramUI/Resources/emoji1.txt`.
- Resource SHA256: `6fecb9b7edbd7ddfb18dc757e1f3de6ce2aaa89f1bdab0251caf28169cd3e58c`.
- The static first-section/base-variant interpretation follows
  `EmojiPagerContentComponentStaticEmoji.swift`; layout metrics are translated
  from the compact branch of `EmojiPagerContentComponent.swift` and native
  static glyph sizing in `EmojiKeyboardItemLayer.swift` at that same revision.
- Upstream is GPL version 2 or later (`GPL-2.0-or-later`); GPLv3 is selected for
  this integration. Full selected terms already in this repository:
  [GPL-3.0](../../../../../../LICENSES/GPL-3.0.txt).
- Telegram's official distribution identifies that license at
  <https://telegram.org/apps/download?setln=en>.

The exact unmodified resource is kept here as generator input. The generated
Swift data embeds 1,651 base emoji in original order, 320 skin-tone groups and 3,491
allowed complete Unicode sequences. Generate with:

```sh
python3 generate-telegram-emoji.py emoji1.txt ZoenEmojiKeyboardData.swift
```

The UIKit content adapter is our implementation of those presentation contracts;
its translated metrics and derived data are attributed, not described as clean
room. Native Unicode glyphs are rendered by iOS; no Telegram raster/vector/font,
animated emoji asset, account service, sticker/GIF provider, or messaging backend
is included. The selected GPLv3 material combines with the project's existing
AGPLv3 terms under section 13; no project-wide license file is changed here.

Source hashes/line anchors and actual qualification limits accompany the private
integration packet. This attribution does not claim full Telegram keyboard
search, category-artwork, skin-picker animation or runtime visual equivalence.
