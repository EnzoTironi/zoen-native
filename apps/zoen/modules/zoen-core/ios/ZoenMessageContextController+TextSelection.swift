import UIKit

// Telegram iOS f1dd7a2: ChatMessageTextBubbleContentNode.updateIsExtractedToContextPreview
// keeps selection on the extracted text; TextSelectionNode performs Quote for its range.
// The existing renderer owns attributed text and source ranges. This controller
// only lends UIKit interaction to the same extracted views for this presentation.
@MainActor
struct ZoenContextTextSelectionState {
  let view: UITextView
  let selectable: Bool
  let editable: Bool
  let range: NSRange
}

extension ZoenMessageContextController {
  func beginTextSelection() {
    guard isCurrent, !closing, !selectingText, let source else { return }
    updateSelection(nil, animated: false)
    heldSelectionAdmitted = false
    selectingText = true
    actions.isHidden = true
    reactionScroll.isHidden = true
    actions.accessibilityElementsHidden = true
    reactionScroll.accessibilityElementsHidden = true
    // Cancel the original held gesture without retiring its source generation.
    // A subsequent native selection gesture must not inherit row activation.
    stage?.contextGesture.cancelAdmission()
    selectionAnnounced = true
    source.selectionPhase(self, active: true)
    reflow(animated: false)
  }

  private func enableTextSelection(in source: ZoenMessageContextSource) {
    var textViews: [UITextView] = []
    func visit(_ view: UIView) {
      if let text = view as? UITextView, !text.isHidden, !text.text.isEmpty { textViews.append(text) }
      for child in view.subviews { visit(child) }
    }
    visit(source.contentHost)
    var enabled = Set(textSelectionInteraction.map { ObjectIdentifier($0.0) })
    let hadText = !textSelectionState.isEmpty
    for text in textViews where !textSelectionState.contains(where: { $0.view === text }) {
      textSelectionState.append(ZoenContextTextSelectionState(view: text,
        selectable: text.isSelectable, editable: text.isEditable, range: text.selectedRange))
      var node: UIView? = text
      while let view = node, view !== preview {
        if enabled.insert(ObjectIdentifier(view)).inserted {
          textSelectionInteraction.append((view, view.isUserInteractionEnabled))
          view.isUserInteractionEnabled = true
        }
        node = view.superview
      }
      text.isEditable = false
      text.isSelectable = true
    }
    // Plain RN Text becomes the existing native markdown leaf in response to
    // the phase event. Its later layout must receive selection interaction too.
    if !hadText, let text = textViews.first {
      text.becomeFirstResponder()
      text.selectAll(nil)
      UIAccessibility.post(notification: .layoutChanged, argument: text)
    }
  }

  func layoutTextSelection(safe: CGRect, original: CGRect, source: ZoenMessageContextSource) {
    enableTextSelection(in: source)
    let size = CGSize(width: source.bounds.width, height: min(source.bounds.height, safe.height))
    preview.layer.removeAllAnimations()
    preview.frame = CGRect(x: min(max(original.minX, safe.minX), safe.maxX - size.width),
      y: min(max(original.minY, safe.minY), safe.maxY - size.height), width: size.width, height: size.height)
    preview.contentSize = source.bounds.size
    source.contentHost.frame = source.bounds
  }

  func restoreTextSelection() {
    guard selectingText else { return }
    selectingText = false
    for state in textSelectionState {
      state.view.resignFirstResponder()
      state.view.isSelectable = state.selectable
      state.view.isEditable = state.editable
      let count = state.view.text.utf16.count
      if state.range.location <= count, state.range.length <= count - state.range.location {
        state.view.selectedRange = state.range
      }
    }
    for (view, enabled) in textSelectionInteraction { view.isUserInteractionEnabled = enabled }
    textSelectionState.removeAll()
    textSelectionInteraction.removeAll()
  }
}
