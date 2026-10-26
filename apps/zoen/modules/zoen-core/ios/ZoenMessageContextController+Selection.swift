import UIKit

extension ZoenMessageContextController {
  func trackHeldTouch(_ point: CGPoint, origin: CGPoint, ended: Bool, cancelled: Bool) {
    guard isCurrent, !closing else { return }
    if cancelled || ended {
      // Reflow can put a menu row under a stationary finger. Only a preceding
      // deliberate Move may admit its hover; Up must never hit-test a new row.
      let selected = ZoenMessageActionHighlight.releasedSelection(hover, admitted: heldSelectionAdmitted, cancelled: cancelled)
      heldSelectionAdmitted = false
      updateSelection(nil, animated: true)
      if let id = selected?.accessibilityIdentifier?.replacingOccurrences(of: "message-context-", with: "") { choose(id) }
      return
    }
    // Telegram f1dd7a2 ContextControllerImpl ContextGesture branch: vertical >4pt.
    // The origin is the original held UITouch in the same window coordinates.
    heldSelectionAdmitted = ZoenMessageActionHighlight.admitHeldMove(point: point, origin: origin, admitted: heldSelectionAdmitted)
    guard heldSelectionAdmitted else { return }
    let target = (reactionButtons + actionButtons).first { button in
      guard let parent = button.superview, !parent.isHidden else { return false }
      let local = button.convert(point, from: sourceWindow)
      return button.bounds.contains(local) && parent.bounds.contains(parent.convert(point, from: sourceWindow))
    }
    updateSelection(target, animated: true)
  }
  @objc func nextAction() { moveFocus(1) }
  @objc func previousAction() { moveFocus(-1) }
  @objc func activateFocused() {
    if let id = hover?.accessibilityIdentifier?.replacingOccurrences(of: "message-context-", with: "") { choose(id) }
  }
  private func moveFocus(_ direction: Int) {
    guard isCurrent, !closing else { return }
    let buttons = reactionButtons + actionButtons
    guard !buttons.isEmpty else { return }
    let index = hover.flatMap { current in buttons.firstIndex(where: { $0 === current }) } ?? (direction > 0 ? -1 : 0)
    let next = buttons[(index + direction + buttons.count) % buttons.count]
    updateSelection(next, animated: false)
    if let scroll = next.superview as? UIScrollView { scroll.scrollRectToVisible(next.frame, animated: !reduceMotion) }
    UIAccessibility.post(notification: .layoutChanged, argument: next)
  }
  func trackActionTouch(_ point: CGPoint, ended: Bool, cancelled: Bool) {
    guard isCurrent, !closing else { return }
    let local = actions.convert(point, from: sourceWindow)
    let index = ZoenMessageActionHighlight.target(at: local, rows: actionButtons.map(\.frame), visibleBounds: actions.bounds)
    let target = index.map { actionButtons[$0] }
    if ended || cancelled {
      let selected = !cancelled && target === hover ? hover : nil
      updateSelection(nil, animated: true)
      if let id = selected?.accessibilityIdentifier?.replacingOccurrences(of: "message-context-", with: "") { choose(id) }
    } else { updateSelection(target, animated: true) }
  }

  func layoutActionHighlight() {
    guard let hover, actionButtons.contains(where: { $0 === hover }) else { return }
    let frame = ZoenMessageActionHighlight.geometry(row: hover.frame, width: actions.bounds.width)
    actionHighlight.frame = frame.rect
    actionHighlight.layer.cornerRadius = frame.radius
  }

  func updateSelection(_ target: UIButton?, animated: Bool) {
    guard hover !== target else { return }
    let old = hover
    hover = target
    if let old, !actionButtons.contains(where: { $0 === old }) {
      old.transform = .identity
      old.backgroundColor = old.isSelected ? UIColor.contextHex(configuration.theme.highlight) : .clear
    }
    let isAction = target.map { candidate in actionButtons.contains(where: { $0 === candidate }) } ?? false
    if let target, !isAction {
      target.backgroundColor = UIColor.contextHex(configuration.theme.highlight)
      if animated && !reduceMotion { target.transform = CGAffineTransform(scaleX: 1.15, y: 1.15) }
    }
    if animated && target != nil && !reduceMotion { UISelectionFeedbackGenerator().selectionChanged() }
    if isAction && (old == nil || !actionButtons.contains(where: { $0 === old })) { layoutActionHighlight() }
    let changes = {
      if isAction { self.layoutActionHighlight() }
      self.actionHighlight.alpha = isAction ? 0.1 : 0
    }
    if animated && !reduceMotion {
      UIView.animate(withDuration: target == nil ? 0.2 : 0.16, delay: 0, options: [.beginFromCurrentState, .curveEaseInOut], animations: changes)
    } else { changes() }
  }
}
