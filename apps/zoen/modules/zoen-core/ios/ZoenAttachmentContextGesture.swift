import UIKit

// Telegram-iOS f1dd7a2, Display/ContextGesture:23–71,74–253 and
// DisplayLinkAnimator's linear duration/from/to animator. UIKit replaces only
// Texture ListView selection cancellation; UITableView deselection is the caller.
@MainActor final class ZoenAttachmentContextGesture: UIGestureRecognizer, UIGestureRecognizerDelegate {
  enum Progress { case begin, update, ended }
  var shouldBegin: ((CGPoint) -> Bool)?
  var activationProgress: ((CGFloat, Progress) -> Void)?
  var activated: ((ZoenAttachmentContextGesture, CGPoint) -> Bool)?
  var externalUpdated: ((UIView?, CGPoint) -> Void)?
  var externalEnded: ((Bool) -> Void)?
  private var delayTimer: Timer?
  private var animator: ContextProgressAnimator?
  private var progress: CGFloat = 0
  private var validated = false

  override init(target: Any?, action: Selector?) {
    super.init(target: target, action: action)
    delegate = self
  }
  func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer) -> Bool {
    !(otherGestureRecognizer is UIPanGestureRecognizer)
  }
  override func reset() {
    super.reset(); endPressedAppearance()
    delayTimer?.invalidate(); delayTimer = nil; animator?.invalidate(); animator = nil
    progress = 0; validated = false; externalUpdated = nil; externalEnded = nil
  }
  override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesBegan(touches, with: event)
    guard let touch = touches.first else { return }
    let location = touch.location(in: view)
    guard touch.location(in: nil).x >= 8, shouldBegin?(location) != false else { state = .failed; return }
    guard delayTimer == nil else { return }
    let timer = Timer(timeInterval: 0.12, repeats: false) { [weak self] _ in
      MainActor.assumeIsolated {
        guard let self, self.delayTimer != nil, self.state == .possible else { return }
        self.validated = true
        self.animator = ContextProgressAnimator(duration: 0.2, update: { [weak self] value in
          guard let self, self.validated else { return }
          self.progress = value; self.activationProgress?(value, .update)
        }, completion: { [weak self] in
          guard let self, self.state == .possible else { return }
          self.delayTimer?.invalidate(); self.animator?.invalidate()
          guard self.activated?(self, location) == true else { self.cancel(); return }
          if let view = self.view {
            if let window = view.window { self.cancelOtherGestures(in: window) }
            self.cancelParentGestures(in: view)
          }
          self.state = .began
        })
        self.activationProgress?(self.progress, .begin)
      }
    }
    delayTimer = timer; RunLoop.main.add(timer, forMode: .common)
  }
  override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesMoved(touches, with: event)
    if let touch = touches.first { externalUpdated?(view, touch.location(in: view)) }
  }
  override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesEnded(touches, with: event)
    endPressedAppearance(); externalEnded?(true)
    delayTimer?.invalidate(); animator?.invalidate(); state = .failed
  }
  override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesCancelled(touches, with: event)
    endPressedAppearance(); externalEnded?(false)
    delayTimer?.invalidate(); animator?.invalidate(); state = .failed
  }
  func cancel() {
    endPressedAppearance(); delayTimer?.invalidate(); animator?.invalidate()
    externalUpdated = nil; externalEnded = nil; state = .failed
  }
  func endPressedAppearance() {
    guard validated else { return }
    progress = 0; validated = false
    delayTimer?.invalidate(); animator?.invalidate()
    activationProgress?(0, .ended)
  }
  private func cancelOtherGestures(in view: UIView) {
    for recognizer in view.gestureRecognizers ?? [] {
      if let recognizer = recognizer as? ZoenAttachmentContextGesture, recognizer !== self { recognizer.cancel() }
      else if recognizer is UITapGestureRecognizer, recognizer.state == .possible { recognizer.state = .failed }
    }
    view.subviews.forEach(cancelOtherGestures)
  }
  private func cancelParentGestures(in view: UIView) {
    for recognizer in view.gestureRecognizers ?? [] where recognizer !== self { recognizer.state = .failed }
    if let parent = view.superview { cancelParentGestures(in: parent) }
  }
}

@MainActor private final class ContextProgressAnimator {
  private var link: SharedDisplayLinkDriver.Link?
  private let duration: Double
  private let start = CACurrentMediaTime()
  private let update: (CGFloat) -> Void
  private let completion: () -> Void
  private var completed = false
  init(duration: Double, update: @escaping (CGFloat) -> Void, completion: @escaping () -> Void) {
    self.duration = duration; self.update = update; self.completion = completion
    link = SharedDisplayLinkDriver.shared.add { [weak self] _ in self?.tick() }
    link?.isPaused = false
  }
  func invalidate() { link?.isPaused = true; link?.invalidate() }
  private func tick() {
    guard !completed else { return }
    let t = min(1, max(0, (CACurrentMediaTime() - start) / duration))
    update(CGFloat(t))
    if abs(t - 1) < Double.ulpOfOne { completed = true; link?.isPaused = true; completion() }
  }
}
