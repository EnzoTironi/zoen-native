import Foundation
import CoreGraphics

enum ZoenMessageActionHighlight {
  static let padding: CGFloat = 10
  static func geometry(row: CGRect, width: CGFloat) -> (rect: CGRect, radius: CGFloat) {
    let rect = CGRect(x: padding, y: row.minY, width: max(0, width - padding * 2), height: row.height)
    return (rect, min(20, rect.height * 0.5))
  }
  static func target(at point: CGPoint, rows: [CGRect], visibleBounds: CGRect) -> Int? {
    guard visibleBounds.contains(point) else { return nil }
    return rows.firstIndex(where: { $0.contains(point) })
  }
  static func admitHeldMove(point: CGPoint, origin: CGPoint, admitted: Bool) -> Bool {
    admitted || abs(point.y - origin.y) > 4
  }
  static func releasedSelection<T>(_ selection: T?, admitted: Bool, cancelled: Bool) -> T? {
    admitted && !cancelled ? selection : nil
  }
}
