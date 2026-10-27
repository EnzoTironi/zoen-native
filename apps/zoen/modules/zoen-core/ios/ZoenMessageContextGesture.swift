import UIKit
import UIKit.UIGestureRecognizerSubclass

/** One recognizer on the permanent Stage, so extraction never changes its ancestor.
 * Before activation it yields to scrolling, edge navigation and nested media pans.
 * Source: Telegram ContextGesture(.12+.2), TapLongTapOrDoubleTap(.16, 2pt).
 */
@MainActor
final class ZoenMessageContextGesture: UIGestureRecognizer, UIGestureRecognizerDelegate {
  weak var source: ZoenMessageContextSource?
  private var observedGeneration: UInt64 = 0
  private var firstPoint = CGPoint.zero
  private var lastPoint = CGPoint.zero
  private var startedAt: CFTimeInterval = 0
  private var displayLink: CADisplayLink?
  private var singleTap: DispatchWorkItem?
  private var firstTapBinding: String?
  private var held = false
  private var doubleTap = false
  private var touchActive = false

  init() {
    super.init(target: nil, action: nil)
    delegate = self
    cancelsTouchesInView = true
    delaysTouchesBegan = false
    delaysTouchesEnded = false
  }
  func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
    guard !UIAccessibility.isVoiceOverRunning, let stage = view as? ZoenMessageContextStage,
          stage.active, stage.controller == nil else { return false }
    var node = touch.view
    var found: ZoenMessageContextSource?
    var path: [UIView] = []
    while let current = node {
      if let candidate = current as? ZoenMessageContextSource { found = candidate; break }
      path.append(current)
      if current === stage { break }
      node = current.superview
    }
    guard let found, found.gestureAdmitted else { cancelAdmission(); return false }
    // The first child is the bubble's own accessible button, whose tap this
    // recognizer implements. Nested links/media/controls keep their own input.
    for current in path where current !== found.children.first {
      if current.accessibilityIdentifier?.hasPrefix("message-context-control:") == true ||
          current is UIControl || current is UITextField ||
          (current as? UITextView).map({ !ZoenMessageContextTextAdmission.allows($0,
            point: touch.location(in: $0), boundary: found.contentHost) }) == true ||
          current.accessibilityTraits.contains(.button) || current.accessibilityTraits.contains(.adjustable) ||
          current.accessibilityTraits.contains(.link) { cancelAdmission(); return false }
    }
    if source !== found { cancelAdmission(); source = found }
    return true
  }
  override func canPrevent(_ preventedGestureRecognizer: UIGestureRecognizer) -> Bool {
    // The Stage's Fabric handler explicitly cancels at admission. Keep native
    // media/scroll recognizers independent before the hold actually activates.
    held || doubleTap
  }
  override func canBePrevented(by preventingGestureRecognizer: UIGestureRecognizer) -> Bool { !held }
  override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesBegan(touches, with: event)
    guard let source, source.gestureAdmitted, touches.count == 1,
          event.allTouches?.filter({ $0.phase != .ended && $0.phase != .cancelled }).count == 1,
          let touch = touches.first else { fail(); return }
    observedGeneration = source.generation
    firstPoint = touch.location(in: nil); lastPoint = firstPoint
    touchActive = true
    if singleTap != nil, firstTapBinding == source.binding, source.handlesTap {
      singleTap?.cancel(); singleTap = nil; firstTapBinding = nil
      doubleTap = true
      source.cancelSurfaceTouch()
      source.dispatch(kind: "double")
      state = .began
      return
    }
    startedAt = CACurrentMediaTime()
    displayLink?.invalidate()
    let link = CADisplayLink(target: self, selector: #selector(progress))
    displayLink = link
    link.add(to: .main, forMode: .common)
  }
  override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesMoved(touches, with: event)
    guard let touch = touches.first, current else { fail(); return }
    lastPoint = touch.location(in: nil)
    if held {
      state = .changed
      source?.presentation?.trackHeldTouch(lastPoint, origin: firstPoint, ended: false, cancelled: false)
    } else if !doubleTap {
      let dx = lastPoint.x - firstPoint.x, dy = lastPoint.y - firstPoint.y
      if dx * dx + dy * dy > 4 { fail() }
    }
  }
  override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesEnded(touches, with: event)
    guard current else { fail(); return }
    touchActive = false
    displayLink?.invalidate(); displayLink = nil
    if let touch = touches.first { lastPoint = touch.location(in: nil) }
    if held {
      source?.presentation?.trackHeldTouch(lastPoint, origin: firstPoint, ended: true, cancelled: false)
      state = .ended
    } else if doubleTap { state = .ended }
    else if let source, source.handlesTap {
      let binding = source.binding, generation = observedGeneration
      firstTapBinding = binding
      let pending = DispatchWorkItem { [weak self, weak source] in
        guard let self, let source, self.singleTap != nil,
              source.gestureAdmitted, source.binding == binding, source.generation == generation else { return }
        self.singleTap = nil; self.firstTapBinding = nil
        source.dispatch(kind: "tap")
      }
      singleTap = pending
      DispatchQueue.main.asyncAfter(deadline: .now() + 0.16, execute: pending)
      // UIKit gets a completed recognized tap; the delayed semantic callback is
      // separately generation-fenced, and a second begin can still consume it.
      state = .ended
    } else { state = .failed }
    source?.resetPress(animated: true)
  }
  override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesCancelled(touches, with: event)
    source?.presentation?.trackHeldTouch(lastPoint, origin: firstPoint, ended: true, cancelled: true)
    fail()
  }
  @objc private func progress() {
    guard current, touchActive, !held, !doubleTap else { displayLink?.invalidate(); displayLink = nil; return }
    let progress = min(1, max(0, (CACurrentMediaTime() - startedAt - 0.12) / 0.2))
    source?.press(CGFloat(progress))
    if progress >= 1 {
      held = true
      displayLink?.invalidate(); displayLink = nil
      source?.cancelSurfaceTouch()
      state = .began
      source?.dispatch(kind: "hold")
    }
  }
  private var current: Bool {
    guard let source else { return false }
    // An admitted live presentation remains valid after the Source placeholder
    // no longer contains the host. Its generation still owns the held touch.
    return source.active && source.generation == observedGeneration && source.window != nil
  }
  func cancelAdmission() {
    singleTap?.cancel(); singleTap = nil; firstTapBinding = nil
    displayLink?.invalidate(); displayLink = nil
    source?.resetPress(animated: false)
    if state == .began || state == .changed { state = .cancelled }
    else if state == .possible && touchActive { state = .failed }
    held = false; doubleTap = false; touchActive = false
  }
  private func fail() {
    cancelAdmission()
    source?.resetPress(animated: true)
    if state == .possible { state = .failed }
  }
  override func reset() {
    super.reset()
    displayLink?.invalidate(); displayLink = nil
    held = false; doubleTap = false; touchActive = false
    source?.resetPress(animated: true)
    // Keep only the bounded .16s semantic tap admission across recognizer reset.
  }
}

// The renderer owns hit-testing of attributed links, code, spoilers, tasks and
// images. Unknown renderers and selectable/editable native text keep their input.
@MainActor
enum ZoenMessageContextTextAdmission {
  static func allows(_ text: UITextView, point: CGPoint, boundary: UIView) -> Bool {
    guard !text.isEditable, !text.isSelectable, text.isDescendant(of: boundary),
          point.x.isFinite, point.y.isFinite, text.bounds.contains(point) else { return false }
    let selector = NSSelectorFromString("zoenMessageContextBodyAtPoint:")
    var ancestor = text.superview
    while let renderer = ancestor, renderer !== boundary {
      if renderer.responds(to: selector) {
        let local = renderer.convert(point, from: text)
        guard renderer.bounds.contains(local),
              let result = renderer.perform(selector, with: NSValue(cgPoint: local))?.takeUnretainedValue() as? NSNumber else { return false }
        return result.boolValue
      }
      ancestor = renderer.superview
    }
    return false
  }
}
