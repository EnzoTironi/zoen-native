# Telegram iOS composer glyphs

These files directly reproduce Telegram-iOS contributors’ artwork from revision `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`. They are not independently drawn assets. The exact source paths, hashes and changes are in UPSTREAM.json.

Telegram identifies its iOS client as GNU GPL version 2 or later at <https://telegram.org/apps>. The GPLv3 option is selected for this integration; see the full selected terms at the project root `LICENSES/GPL-3.0.txt`. Zoen's combined project terms remain separately described at the project root. No separate permissive asset license or individual copyright header was found in these five pinned imagesets. Attribution is to Telegram-iOS contributors.

The eight @2x/@3x PNG files and send.pdf are exact upstream bytes. The send@2x/@3x PNGs are deterministic PDFium renderings of that PDF on a transparent 34pt canvas, centering its 30pt original at a 2pt offset, matching the source-observed placement contract. They may differ from UIKit raster antialiasing. Tint is a runtime host choice under the documented theme branch. No Telegram brand logo or account/network integration is included.
