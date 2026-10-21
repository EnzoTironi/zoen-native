import Foundation
import CoreGraphics

// Telegram-iOS f1dd7a2, ContextControllerExtractedPresentationNode:
// 734–759,1100–1113,1189–1203,1267–1328,1420–1450,1550–1588,1695.
// File's source: keepInPlace=false, no reaction/position locks, no portal surface.
enum ZoenAttachmentExtractedMenuLayout {
  struct Geometry {
    let content: CGRect
    let menu: CGRect
    let contentSize: CGSize
    let initialScrollY: CGFloat
    let contentOpeningOffset: CGFloat
    let menuOpeningOffset: CGPoint
  }
  static func geometry(viewport: CGSize, source: CGRect, menuSize: CGSize,
                       statusBarHeight: CGFloat, intrinsicBottom: CGFloat) -> Geometry {
    let top = statusBarHeight + 8
    let bottom = 10 + intrinsicBottom
    var content = source
    content.origin.y = max(content.minY, top)
    let combinedHeight = content.height + 7 + menuSize.height
    if content.minY + combinedHeight > viewport.height - bottom {
      content.origin.y = viewport.height - bottom - combinedHeight
    }
    content.origin.y = max(top, content.minY)
    var x = content.midX < viewport.width / 2
      ? content.minX + 6 - 4 : content.maxX - 6 - menuSize.width - 1
    x = max(12, min(viewport.width - 12 - menuSize.width, x))
    let menu = CGRect(x: x, y: content.maxY + 7, width: menuSize.width, height: menuSize.height)
    let contentSize = CGSize(width: viewport.width, height: menu.maxY + bottom)
    let initialScrollY = max(0, contentSize.height - viewport.height)
    let travel = content.maxY - initialScrollY - source.maxY
    return Geometry(content: content, menu: menu, contentSize: contentSize,
      initialScrollY: initialScrollY,
      contentOpeningOffset: -travel,
      menuOpeningOffset: CGPoint(x: 0, y: -travel - menuSize.height / 2 - 7))
  }
}
