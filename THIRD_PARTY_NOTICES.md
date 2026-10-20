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
