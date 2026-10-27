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
    // Preserve the original held segment/paragraph across extraction. A retired
    // text leaf never retargets another segment; plain RN Text may mount only
    // one unambiguous native leaf in response to the selection phase.
    let target = ZoenMessageHeldParagraph.target(in: textViews, held: heldText, hadHeld: hadHeldText)
    if !textSelectionInitialized, let text = target,
       let range = ZoenMessageHeldParagraph.range(in: text, point: hadHeldText ? heldTextPoint : nil) {
      textSelectionInitialized = true
      text.becomeFirstResponder()
      text.selectedRange = range
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

// UIKit supplies the caret offset in the original selected text view. NSString
// paragraph boundaries use that same UTF-16 domain; never search duplicate text.
@MainActor
enum ZoenMessageHeldParagraph {
  static func target(in views: [UITextView], held: UITextView?, hadHeld: Bool) -> UITextView? {
    if hadHeld { return held.flatMap { text in views.contains(where: { $0 === text }) ? text : nil } }
    return views.count == 1 ? views.first : nil
  }
  static func range(in text: UITextView, point: CGPoint?) -> NSRange? {
    let value = text.text as NSString
    guard value.length > 0 else { return nil }
    let offset: Int
    if let point {
      guard point.x.isFinite, point.y.isFinite, let position = text.closestPosition(to: point) else { return nil }
      offset = min(max(0, text.offset(from: text.beginningOfDocument, to: position)), value.length - 1)
    } else { offset = 0 }
    var paragraph = value.paragraphRange(for: NSRange(location: offset, length: 0))
    while paragraph.length > 0,
          let scalar = UnicodeScalar(value.character(at: NSMaxRange(paragraph) - 1)),
          CharacterSet.newlines.contains(scalar) { paragraph.length -= 1 }
    guard paragraph.length > 0, point != nil || paragraph.length == value.length else { return nil }
    return paragraph
  }
}
