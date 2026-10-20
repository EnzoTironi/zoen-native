import Foundation

/// Presentation admission only; never receipt/delivery authority.
struct ZoenMessageReceiptPresentation {
  enum Shape: String { case clock, single, double }
  struct Change: Equatable {
    let resetAnimations: Bool
    let revealFull: Bool
    let revealPartial: Bool
  }
  private var owner = ""
  private var shape: Shape?
  private var visible = false
  private var wasAttached = false
  private var needsBaselineProps = false

  mutating func windowChanged(visible: Bool) {
    self.visible = visible
    if visible { wasAttached = true }
    else if wasAttached { needsBaselineProps = true }
  }
  mutating func invalidate() {
    owner = ""; shape = nil
    if wasAttached { needsBaselineProps = true }
  }
  mutating func accept(owner nextOwner: String, shape nextShape: Shape, propsDelivered: Bool) -> Change {
    let sameOwner = !nextOwner.isEmpty && owner == nextOwner
    let changed = shape != nextShape
    let admitted = visible && !needsBaselineProps && sameOwner && shape != nil
    let result = Change(resetAnimations: !sameOwner || changed,
                        revealFull: admitted && changed && shape == .clock && nextShape != .clock,
                        revealPartial: admitted && changed && nextShape == .double)
    owner = nextOwner; shape = nextShape
    // A reattached view's first real props delivery is baseline-only, even if
    // layout/didMoveToWindow already applied the stale pre-detach properties.
    if visible && propsDelivered { needsBaselineProps = false }
    return result
  }
}
