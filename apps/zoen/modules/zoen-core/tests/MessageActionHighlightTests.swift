import Foundation
import CoreGraphics

@main struct MessageActionHighlightTests {
  static func main() {
    let rows = [CGRect(x: 0, y: 10, width: 240, height: 44), CGRect(x: 0, y: 54, width: 240, height: 60)]
    let geometry = ZoenMessageActionHighlight.geometry(row: rows[0], width: 240)
    precondition(geometry.rect == CGRect(x: 10, y: 10, width: 220, height: 44))
    precondition(geometry.radius == 20)
    precondition(ZoenMessageActionHighlight.geometry(row: CGRect(x: 0, y: 54, width: 240, height: 24), width: 240).radius == 12)
    precondition(ZoenMessageActionHighlight.geometry(row: rows[0], width: 12).rect.width == 0)
    let visible = CGRect(x: 0, y: 0, width: 240, height: 124)
    func target(_ x: CGFloat, _ y: CGFloat) -> Int? {
      ZoenMessageActionHighlight.target(at: CGPoint(x: x, y: y), rows: rows, visibleBounds: visible)
    }
    precondition(target(20, 9) == nil && target(20, 114) == nil, "top and bottom padding cannot select")
    precondition(target(1, 10) == 0 && target(239, 53) == 0, "inset is visual; row remains a full width target")
    precondition(target(20, 54) == 1 && target(240, 60) == nil)
    precondition(ZoenMessageActionHighlight.target(at: CGPoint(x: 20, y: 54), rows: rows, visibleBounds: CGRect(x: 0, y: 60, width: 240, height: 40)) == nil, "clipped rows cannot select")
    let origin = CGPoint(x: 10, y: 10)
    precondition(!ZoenMessageActionHighlight.admitHeldMove(point: CGPoint(x: 100, y: 14), origin: origin, admitted: false))
    precondition(ZoenMessageActionHighlight.admitHeldMove(point: CGPoint(x: 10, y: 5), origin: origin, admitted: false))
    precondition(ZoenMessageActionHighlight.admitHeldMove(point: origin, origin: origin, admitted: true))
    precondition(ZoenMessageActionHighlight.releasedSelection(1, admitted: false, cancelled: false) == nil, "stationary release cannot admit a row after reflow")
    precondition(ZoenMessageActionHighlight.releasedSelection(1, admitted: true, cancelled: true) == nil)
    precondition(ZoenMessageActionHighlight.releasedSelection(1, admitted: true, cancelled: false) == 1)
    precondition(ZoenMessageActionHighlight.releasedSelection(nil as Int?, admitted: true, cancelled: false) == nil)
    print("MessageActionHighlightTests: passed geometry, clipping, row transition, held admission and cancellation")
  }
}
