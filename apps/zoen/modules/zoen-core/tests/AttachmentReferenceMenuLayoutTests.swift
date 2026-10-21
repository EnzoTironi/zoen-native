import Foundation
import CoreGraphics

/// Execute with the exact production Foundation-only layout/lifecycle source.
@main struct AttachmentReferenceMenuLayoutTests {
  static func main() {
    typealias Layout = ZoenAttachmentReferenceMenuLayout
    let right = Layout.geometry(viewport: CGSize(width: 402, height: 874),
      source: CGRect(x: 342, y: 80, width: 44, height: 44),
      measuredSize: CGSize(width: 220, height: 62), intrinsicBottom: 34)
    precondition(right.source == CGRect(x: 340, y: 80, width: 53, height: 44))
    precondition(right.menu == CGRect(x: 166, y: 131, width: 220, height: 62))
    precondition(right.initialScrollY == 0 && right.originOffset.y == -38)
    let left = Layout.geometry(viewport: CGSize(width: 402, height: 874),
      source: CGRect(x: 16, y: 80, width: 44, height: 44),
      measuredSize: CGSize(width: 240, height: 62), intrinsicBottom: 34)
    precondition(left.menu.minX == 16 && left.menu.minY == 131)
    var cases = 0
    for size in [CGSize(width: 320, height: 568), CGSize(width: 375, height: 667), CGSize(width: 402, height: 874), CGSize(width: 874, height: 402)] {
      for sourceX: CGFloat in [0, 16, size.width / 2, size.width - 44] {
        for sourceY: CGFloat in [20, 80, size.height - 100] {
          for height: CGFloat in [62, 124, 620] {
            let result = Layout.geometry(viewport: size, source: CGRect(x: sourceX, y: sourceY, width: 44, height: 44), measuredSize: CGSize(width: 240, height: height), intrinsicBottom: 34)
            precondition(result.menu.minX >= 16 && result.menu.maxX <= size.width - 16)
            precondition(result.menu.minY == sourceY + 44 + 7)
            precondition(result.initialScrollY == max(0, result.menu.maxY + 44 - size.height))
            precondition(result.menu.maxY - result.initialScrollY <= size.height - 44)
            cases += 1
          }
        }
      }
    }
    precondition(Layout.width(textWidths: [100], iconWidths: [24], available: 402) == 220)
    precondition(Layout.width(textWidths: [180], iconWidths: [40], available: 402) == 266)
    precondition(Layout.width(textWidths: [1000], iconWidths: [40], available: 320) == 288)
    for interrupted in [false, true] {
      var owner = ZoenAttachmentReferenceMenuLifecycle()
      precondition(!owner.isInteractive && owner.present() && !owner.present())
      precondition(owner.isInteractive && owner.dismiss() && !owner.dismiss())
      if interrupted { precondition(owner.remove()) }
      precondition(!owner.isInteractive && !owner.present() && !owner.dismiss())
      if !interrupted { precondition(owner.remove()) }
      precondition(!owner.remove() && !owner.present())
    }
    print("PASS: exact source anchors, \(cases) production reference layouts, bounded title/icon sizing and interruption/single-action lifecycle")
  }
}
