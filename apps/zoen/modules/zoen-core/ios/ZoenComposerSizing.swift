import UIKit

/** Read-only fitting of the exact mounted React Native editor. */
@MainActor
enum ZoenComposerSizing {
  static func measure(
    root: UIView,
    tag: Int,
    expectedText: String,
    expectedLabel: String,
    expectedPlaceholder: String,
    width: Double,
    expectedFontSize: Double,
    nonce: Int
  ) -> [String: Double]? {
    guard tag > 0, nonce >= 0, nonce <= 9_007_199_254_740_991,
          width.isFinite, width > 0,
          expectedFontSize.isFinite, expectedFontSize > 0,
          !expectedLabel.isEmpty, let window = root.window else { return nil }
    // Never search the window or sibling fields. Ambiguous or unusually large
    // subtrees fail closed rather than measuring a different composer.
    var pending = [root]
    var editor: UITextView?
    var visited = 0
    while let view = pending.popLast() {
      visited += 1
      guard visited <= 128 else { return nil }
      if let textView = view as? UITextView {
        guard editor == nil else { return nil }
        editor = textView
      }
      pending.append(contentsOf: view.subviews)
    }
    guard let editor, editor.window === window,
          editor.text == expectedText else { return nil }
    // RN's real multiline editor appends its placeholder to the accessible
    // label while empty. A labeled tagged container keeps the base label.
    let rootLabel = root.accessibilityLabel
    if let rootLabel, !rootLabel.isEmpty {
      guard rootLabel == expectedLabel else { return nil }
    } else {
      let editorLabel = expectedText.isEmpty && !expectedPlaceholder.isEmpty
        ? "\(expectedLabel) \(expectedPlaceholder)" : expectedLabel
      guard editor.accessibilityLabel == editorLabel else { return nil }
    }
    let actualWidth = Double(editor.bounds.width)
    guard actualWidth.isFinite, actualWidth > 0,
          abs(actualWidth - width) <= 1 else { return nil }
    let attributedFont: UIFont? = if let attributed = editor.attributedText, attributed.length > 0 {
      attributed.attribute(.font, at: 0, effectiveRange: nil) as? UIFont
    } else {
      nil
    }
    guard let font = attributedFont ?? editor.font ?? editor.typingAttributes[.font] as? UIFont else {
      return nil
    }
    let actualFontSize = Double(font.pointSize)
    guard actualFontSize.isFinite,
          abs(actualFontSize - expectedFontSize) <= 0.5 else { return nil }
    // UITextView (including RN's override) accounts for its real paragraph
    // attributes, wrapping, placeholder and insets. Do not add padding again.
    let fitting = editor.sizeThatFits(
      CGSize(width: editor.bounds.width, height: .greatestFiniteMagnitude)
    )
    guard fitting.height.isFinite, fitting.height > 0 else { return nil }
    return [
      "tag": Double(tag),
      "nonce": Double(nonce),
      "width": actualWidth,
      "height": Double(fitting.height),
      "fontSize": actualFontSize,
    ]
  }
}
