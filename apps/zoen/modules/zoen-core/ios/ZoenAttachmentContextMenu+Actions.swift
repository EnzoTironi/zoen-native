import UIKit

@MainActor extension ZoenAttachmentContextMenu {
  @objc func dragged(_ recognizer: UIGestureRecognizer) {
    switch recognizer.state {
    case .began, .changed:
      highlight(at: recognizer.location(in: surface))
    case .ended:
      highlight(at: recognizer.location(in: surface))
      let index = highlightedIndex; setHighlighted(nil, animated: true, feedback: false)
      if let index { choose(index) }
    case .cancelled, .failed: setHighlighted(nil, animated: true, feedback: false)
    default: break
    }
  }
  func highlight(at point: CGPoint?) {
    guard lifecycle.isInteractive, isValid() else { return }
    let index = point.flatMap { point in rows.indices.first { rows[$0].frame.contains(point) && actions[$0].enabled } }
    setHighlighted(index, animated: true)
  }
  // ContextControllerImpl:270–340 continues the source long press into the
  // action list only after 4pt of vertical movement. Lift-in-place leaves it open.
  func continueSourceGesture(view: UIView?, point: CGPoint) {
    guard lifecycle.isInteractive else { return }
    let local = convert(point, from: view)
    if externalInitialPoint == nil { externalInitialPoint = local }
    if let initial = externalInitialPoint, abs(local.y - initial.y) > 4 { externalMoved = true }
    if externalMoved { highlight(at: convert(local, to: surface)) }
  }
  func endSourceGesture(performAction: Bool) {
    let index = externalMoved ? highlightedIndex : nil
    setHighlighted(nil, animated: true, feedback: false)
    if performAction, let index { choose(index) }
  }
  private func setHighlighted(_ index: Int?, animated: Bool, feedback: Bool = true) {
    guard highlightedIndex != index else { return }
    highlightedIndex = index
    if feedback { haptic.selectionChanged() }
    updateHighlight(animated: animated)
  }
  func updateHighlight(animated: Bool) {
    let initialHighlight = highlightedIndex != nil && highlight.alpha == 0
    if initialHighlight, let index = highlightedIndex {
      let row = rows[index].frame
      highlight.frame = CGRect(x: 10, y: row.minY, width: max(0, row.width - 20), height: row.height)
      highlight.layer.cornerRadius = min(20, row.height / 2)
    }
    let duration: Double = highlightedIndex == nil || initialHighlight ? 0.2 : 0.16
    let apply = {
      if let index = self.highlightedIndex {
        let row = self.rows[index].frame
        self.highlight.frame = CGRect(x: 10, y: row.minY, width: max(0, row.width - 20), height: row.height)
        self.highlight.layer.cornerRadius = min(20, row.height / 2)
        self.highlight.alpha = 0.1
      } else { self.highlight.alpha = 0 }
    }
    if animated && !reducedMotion() {
      UIView.animate(withDuration: duration, delay: 0, options: [.beginFromCurrentState, .allowUserInteraction, .curveEaseInOut], animations: apply)
    } else { UIView.performWithoutAnimation(apply) }
  }
  func choose(_ index: Int) {
    guard lifecycle.isInteractive, actions.indices.contains(index), actions[index].enabled,
          let window = admittedWindow, presentation?.isAttached(in: window) == true,
          source.view?.window === window, sourceIsVisible, isValid() else { invalidateIfNeeded(); return }
    let action = actions[index]
    isReturningAfterAction = extraction != nil
    if action.invocation == .afterContentReturns, extraction != nil { returnedAction = action.perform }
    // MediaPickerScreen:3174–3181 and AttachmentFileController:724–734 start
    // dismissal, then act immediately. Restore-before-handoff is only for the
    // explicit Zoen Preview extension, absent from upstream's normal File menu.
    dismiss(animated: true)
    if action.invocation == .afterDismissalBegins || extraction == nil { action.perform() }
  }
  func dismiss(animated: Bool) {
    guard lifecycle.dismiss() else { return }
    presentation?.beginWindowPolicyDismissal()
    isUserInteractionEnabled = false; setHighlighted(nil, animated: false, feedback: false)
    guard animated, window != nil else { remove(); return }
    let layer = glassContainer.layer
    let visible = layer.presentation()
    let alpha = visible?.opacity ?? layer.opacity
    let scale = (visible?.value(forKeyPath: "transform.scale") as? NSNumber)?.doubleValue ?? 1
    let offset = visible.map { CGPoint(x: $0.position.x - layer.position.x, y: $0.position.y - layer.position.y) } ?? .zero
    var targetOffset = geometry?.originOffset ?? .zero
    if let source = source.view, source.window === admittedWindow {
      var sourceRect = source.convert(source.bounds, to: self).insetBy(dx: -2, dy: 0)
      sourceRect.size.width += 5
      let center = glassContainer.convert(CGPoint(x: glassContainer.bounds.midX, y: glassContainer.bounds.midY), to: self)
      targetOffset = CGPoint(x: extraction == nil ? sourceRect.midX - center.x : 0, y: sourceRect.maxY - center.y)
    }
    layer.removeAllAnimations()
    let backdropOpacity = CGFloat(backdrop?.layer.presentation()?.opacity ?? 1)
    backdrop?.layer.removeAllAnimations()
    backdrop?.layer.animateAlpha(from: backdropOpacity, to: 0,
      duration: ZoenAttachmentReferenceMenuLayout.closingDuration, removeOnCompletion: false)
    if let extraction, let source = source.view {
      extraction.beginReturn(animated: animated)
      let target = source.convert(source.bounds, to: scroll)
      let fromY = extractedStage.layer.presentation()?.position.y ?? extractedStage.layer.position.y
      extractedStage.layer.removeAllAnimations()
      extractedStage.layer.position.y = target.midY
      if !reducedMotion() {
        extractedStage.layer.animate(from: (fromY - target.midY) as NSNumber, to: 0 as NSNumber,
          keyPath: "position.y", timingFunction: CAMediaTimingFunctionName.easeInEaseOut.rawValue,
          duration: ZoenAttachmentReferenceMenuLayout.closingDuration, removeOnCompletion: false, additive: true)
      }
    }
    layer.animateAlpha(from: CGFloat(alpha), to: 0, duration: ZoenAttachmentReferenceMenuLayout.closingDuration, removeOnCompletion: false)
    let finish: (Bool) -> Void = { [weak self] completed in self?.remove(allowReturnedAction: completed) }
    if reducedMotion() {
      layer.animateAlpha(from: CGFloat(alpha), to: 0, duration: ZoenAttachmentReferenceMenuLayout.closingDuration,
        removeOnCompletion: false, completion: finish)
    } else {
      layer.animate(from: scale as NSNumber, to: ZoenAttachmentReferenceMenuLayout.closedScale as NSNumber,
        keyPath: "transform.scale", timingFunction: CAMediaTimingFunctionName.easeInEaseOut.rawValue,
        duration: ZoenAttachmentReferenceMenuLayout.closingDuration, removeOnCompletion: false, completion: finish)
      layer.animate(from: NSValue(cgPoint: offset), to: NSValue(cgPoint: targetOffset), keyPath: "position",
        timingFunction: CAMediaTimingFunctionName.easeInEaseOut.rawValue, duration: ZoenAttachmentReferenceMenuLayout.closingDuration,
        removeOnCompletion: false, additive: true)
    }
  }
  /// Retirement interrupts an in-flight opening/closing and restores ownership
  /// synchronously. A superseded CA completion cannot execute any action.
  func retire() { remove() }
  func remove(allowReturnedAction: Bool = false) {
    guard lifecycle.remove() else { return }
    let action: (() -> Void)?
    if allowReturnedAction, let window = admittedWindow, presentation?.isAttached(in: window) == true,
       source.view?.window === window, sourceIsVisible, isValid() { action = returnedAction }
    else { action = nil }
    returnedAction = nil; isReturningAfterAction = false
    isUserInteractionEnabled = false
    observers.forEach(NotificationCenter.default.removeObserver); observers.removeAll()
    glassContainer.layer.removeAllAnimations(); highlight.layer.removeAllAnimations()
    extractedStage.layer.removeAllAnimations(); backdrop?.layer.removeAllAnimations()
    extraction?.restore()
    presentation?.remove(); presentation = nil
    didDismiss(self)
    if let source = source.view, source.window === admittedWindow, sourceIsVisible, isValid() {
      UIAccessibility.post(notification: .layoutChanged, argument: source)
    }
    action?()
  }
}
