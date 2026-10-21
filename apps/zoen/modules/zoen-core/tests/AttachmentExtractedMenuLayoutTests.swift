import Foundation
import CoreGraphics

@main struct AttachmentExtractedMenuLayoutTests {
  static func main() {
    typealias Layout = ZoenAttachmentExtractedMenuLayout
    let bottom = Layout.geometry(viewport: CGSize(width: 402, height: 874),
      source: CGRect(x: 16, y: 780, width: 370, height: 60), menuSize: CGSize(width: 220, height: 104),
      statusBarHeight: 62, intrinsicBottom: 34)
    precondition(bottom.content == CGRect(x: 16, y: 659, width: 370, height: 60))
    precondition(bottom.menu == CGRect(x: 159, y: 726, width: 220, height: 104))
    precondition(bottom.contentOpeningOffset == 121 && bottom.menuOpeningOffset == CGPoint(x: 0, y: 62))
    var cases = 0
    for viewport in [CGSize(width: 320, height: 568), CGSize(width: 402, height: 874), CGSize(width: 874, height: 402)] {
      for top: CGFloat in [-60, 20, 100, viewport.height - 80] {
        for rowHeight: CGFloat in [52, 64] {
          for menuHeight: CGFloat in [62, 104, 620] {
            for scale: CGFloat in [1, 0.974] {
              let source = CGRect(x: 16, y: top, width: (viewport.width - 32) * scale, height: rowHeight * scale)
              let result = Layout.geometry(viewport: viewport, source: source, menuSize: CGSize(width: 240, height: menuHeight), statusBarHeight: 62, intrinsicBottom: 34)
              precondition(result.content.minX == source.minX && result.content.size == source.size)
              precondition(result.content.minY >= 70)
              precondition(result.menu.minY == result.content.maxY + 7)
              precondition(result.menu.minX >= 12 && result.menu.maxX <= viewport.width - 12)
              precondition(result.menu.maxY - result.initialScrollY <= viewport.height - 44)
              precondition(abs(result.content.minY - result.initialScrollY + result.contentOpeningOffset - source.minY) < 0.000001)
              precondition(result.menuOpeningOffset.x == 0)
              cases += 1
            }
          }
        }
      }
    }
    print("PASS: pinned bottom-row travel anchor + \(cases) production extracted layouts, scaled rows, clipping/overflow and distinct reference alignment")
  }
}
