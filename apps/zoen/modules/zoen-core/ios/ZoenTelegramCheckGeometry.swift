// SPDX-License-Identifier: GPL-2.0-or-later
// Derived procedural glyph data from TelegramMessenger/Telegram-iOS contributors.
// Source: submodules/TelegramPresentationData/Sources/PresentationThemeEssentialGraphics.swift,
// generateCheckImage(partial:color:width:), lines8–24, revisionf1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// Source SHA256:d9373e9220ce3c245b9b34ad296677556eb76505bedfa76a4929f74da3769ed2.
// The fractional SVG control-point data is translated here, not claimed to be clean-room artwork.
// No individual copyright header is provided by that pinned upstream file; attribution is to its contributors.
// This derived portion retains GPLv2-or-later; integration selects its GPLv3 option.
// See repository LICENSES/GPL-3.0.txt and THIRD_PARTY_NOTICES.md (root-owned licensing commit).
import CoreGraphics

enum ZoenTelegramCheckGeometry {
  enum Segment {
    case move(CGPoint)
    case line(CGPoint)
    case cubic(end: CGPoint, first: CGPoint, second: CGPoint)
  }
  static func segments(partial: Bool) -> [Segment] {
    if partial { return [.move(CGPoint(x: 0.5, y: 7)), .line(CGPoint(x: 7, y: 0))] }
    return [
      .move(CGPoint(x: 0, y: 4)),
      .line(CGPoint(x: 2.95157047, y: 6.95157047)),
      .line(CGPoint(x: 2.95157047, y: 6.95157047)),
      .cubic(end: CGPoint(x: 3.04490857, y: 6.95157047), first: CGPoint(x: 2.97734507, y: 6.97734507), second: CGPoint(x: 3.01913396, y: 6.97734507)),
      .cubic(end: CGPoint(x: 3.04660389, y: 6.9498112), first: CGPoint(x: 3.04548448, y: 6.95099456), second: CGPoint(x: 3.04604969, y: 6.95040803)),
      .line(CGPoint(x: 9.5, y: 0))
    ]
  }
}
