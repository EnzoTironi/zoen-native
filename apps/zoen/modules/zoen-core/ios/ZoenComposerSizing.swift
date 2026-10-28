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
    guard let editor, editor.window === window else { return nil }
    let canonicalSelector = NSSelectorFromString("zoenComposerCanonicalText")
    let canonicalInput = root.responds(to: canonicalSelector)
    if canonicalInput {
      guard let canonical = root.perform(canonicalSelector)?.takeUnretainedValue() as? String,
            canonical == expectedText else { return nil }
    } else {
      guard editor.text == expectedText else { return nil }
    }
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
    let actualWidth = Double(canonicalInput ? root.bounds.width : editor.bounds.width)
    guard actualWidth.isFinite, actualWidth > 0,
          abs(actualWidth - width) <= 1,
          editor.bounds.width.isFinite, editor.bounds.width > 0 else { return nil }
    var containerInsets: CGFloat = 0
    if canonicalInput {
      // ENRM uses RCTViewComponentView.contentView: Fabric puts padding on
      // the host, outside its one UITextView. Measure its actual inner width
      // and add only that real outer vertical geometry.
      let frame = editor.convert(editor.bounds, to: root)
      guard frame.minX >= -0.5, frame.minY >= -0.5,
            frame.maxX <= root.bounds.maxX + 0.5,
            frame.maxY <= root.bounds.maxY + 0.5 else { return nil }
      containerInsets = max(0, root.bounds.height - frame.height)
    }
    let actualFontSize: Double
    if canonicalInput {
      // Heading/code runs intentionally differ from the body font. Validate
      // the same native formatter's base size, not the first selected style.
      let baseFontSelector = NSSelectorFromString("zoenComposerBaseFontSize")
      guard root.responds(to: baseFontSelector),
            let size = root.perform(baseFontSelector)?.takeUnretainedValue() as? NSNumber else { return nil }
      actualFontSize = size.doubleValue
    } else {
      let attributedFont: UIFont? = if let attributed = editor.attributedText, attributed.length > 0 {
        attributed.attribute(.font, at: 0, effectiveRange: nil) as? UIFont
      } else {
        nil
      }
      guard let font = attributedFont ?? editor.font ?? editor.typingAttributes[.font] as? UIFont else {
        return nil
      }
      actualFontSize = Double(font.pointSize)
    }
    guard actualFontSize.isFinite,
          abs(actualFontSize - expectedFontSize) <= 0.5 else { return nil }
    let contentHeight: Double
    if canonicalInput {
      // Use the renderer's own measureSize path. Its typing attributes measure
      // empty and trailing-newline paragraphs; UITextView.sizeThatFits can use
      // the unstyled empty editor font instead. Host padding stays outside it.
      let fittingSelector = NSSelectorFromString("zoenComposerFittingHeight")
      guard root.responds(to: fittingSelector),
            let height = root.perform(fittingSelector)?.takeUnretainedValue() as? NSNumber else { return nil }
      contentHeight = height.doubleValue
    } else {
      contentHeight = Double(editor.sizeThatFits(
        CGSize(width: editor.bounds.width, height: .greatestFiniteMagnitude)
      ).height)
    }
    guard contentHeight.isFinite, contentHeight > 0 else { return nil }
    return [
      "tag": Double(tag),
      "nonce": Double(nonce),
      "width": actualWidth,
      "height": contentHeight + Double(containerInsets),
      "fontSize": actualFontSize,
    ]
  }
}
