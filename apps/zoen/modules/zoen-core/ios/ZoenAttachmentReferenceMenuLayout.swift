import Foundation
import CoreGraphics

// Telegram-iOS f1dd7a2, GPL-2.0-or-later (GPLv3 selected):
// ContextControllerExtractedPresentationNode:734–759,1003–1016,1267–1328,
// 1420–1454,1469–1470,1660–1704,1757–1771,2103–2136;
// ContextControllerActionsStackNode:212–239,551–575,993–1044,2024–2044.
// Plain reference/list branch only; no extracted row or opt-in liquid morph.
enum ZoenAttachmentReferenceMenuLayout {
  static let openingDuration = 0.42
  static let openingOpacityDuration = 0.05
  static let springDamping: CGFloat = 104
  static let closedScale: CGFloat = 0.01
  static let closingDuration = 0.2
  static let textInset: CGFloat = 18
  static let iconInset: CGFloat = 20
  static let iconColumn: CGFloat = 32
  static let iconSpacing: CGFloat = 8
  static let verticalInset: CGFloat = 11
  static let listInset: CGFloat = 10
  static let sourceSpacing: CGFloat = 7
  static let edgeInset: CGFloat = 16

  struct Geometry {
    let source: CGRect
    let menu: CGRect
    let contentSize: CGSize
    let initialScrollY: CGFloat
    let originOffset: CGPoint
  }

  static func geometry(viewport: CGSize, source: CGRect, measuredSize: CGSize,
                       intrinsicBottom: CGFloat) -> Geometry {
    var content = source.insetBy(dx: -2, dy: 0)
    content.size.width += 5
    let width = min(measuredSize.width, max(0, viewport.width - edgeInset * 2))
    var x = content.midX < viewport.width / 2
      ? content.minX + 6 - 4 : floor(content.midX - width / 2)
    x = max(edgeInset, min(viewport.width - edgeInset - width, x))
    let menu = CGRect(x: x, y: content.maxY + sourceSpacing, width: width, height: measuredSize.height)
    let contentSize = CGSize(width: viewport.width, height: menu.maxY + 10 + max(0, intrinsicBottom))
    return Geometry(source: content, menu: menu, contentSize: contentSize,
      initialScrollY: max(0, contentSize.height - viewport.height),
      originOffset: CGPoint(x: content.midX - menu.midX,
        y: -menu.height / 2 - sourceSpacing))
  }

  static func width(textWidths: [CGFloat], iconWidths: [CGFloat], available: CGFloat) -> CGFloat {
    var width: CGFloat = 220
    for (index, textWidth) in textWidths.enumerated() {
      let iconWidth = iconWidths.indices.contains(index) ? iconWidths[index] : 0
      width = max(width, textInset + textWidth + (iconWidth > 0
        ? max(iconColumn, iconWidth) + iconInset + iconSpacing : textInset))
    }
    return min(width, max(0, available - edgeInset * 2))
  }
}

/// One menu presentation, one dismissal, at most one action. The native caller
/// retains the separate account/revision/selection/caption admission authority.
struct ZoenAttachmentReferenceMenuLifecycle {
  enum Phase { case idle, presented, dismissing, removed }
  private(set) var phase: Phase = .idle
  var isInteractive: Bool { phase == .presented }
  mutating func present() -> Bool {
    guard phase == .idle else { return false }
    phase = .presented; return true
  }
  mutating func dismiss() -> Bool {
    guard phase == .presented else { return false }
    phase = .dismissing; return true
  }
  @discardableResult mutating func remove() -> Bool {
    guard phase != .removed else { return false }
    phase = .removed; return true
  }
}
