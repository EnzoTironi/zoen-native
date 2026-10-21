# Telegram File and Audio attachment port

The source excerpts and original Search PDF identified in `UPSTREAM_SOURCES.json`
and `UPSTREAM_ASSETS.json` derive from TelegramMessenger/Telegram-iOS contributors
at revision `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`. Official source URLs,
SHA256 values and relevant ranges are retained in the manifests.

Telegram-iOS is GPL-2.0-or-later. Zoen selects GPLv3 for this integration,
combined with Zoen under AGPLv3 section 13. The complete selected terms are in
`Resources/Notices/Telegram-GPL-3.0.txt`, the repository `LICENSES/GPL-3.0.txt`
and root `LICENSE`. Copyright Telegram-iOS contributors.

This UIKit adaptation preserves the original fixed document vectors/colors,
row geometry, selection offset, action geometry, empty-state layout and search
control paths/metrics. It reuses the existing pinned original glass, glyphs and
Files animation. UIKit text/table reuse replaces AsyncDisplayKit/ListView.
Zoen supplies authorized bounded query rows and retains all file, identity,
preview, encryption and draft authority. No Telegram account services,
Postbox database, remote music provider, profile collection or file transport
is imported. This is a bounded functional slice, not a full-controller or
pixel-parity claim; native device qualification is separate.
