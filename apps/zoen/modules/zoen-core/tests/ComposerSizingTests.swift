import UIKit
import XCTest
@testable import ZoenCore

@MainActor
private final class CanonicalComposerRoot: UIView {
  var baseFontSize = 17.0
  var canonicalText = "# título"
  var fittingHeight: NSNumber?
  @objc func zoenComposerCanonicalText() -> String { canonicalText }
  @objc func zoenComposerBaseFontSize() -> NSNumber { NSNumber(value: baseFontSize) }
  @objc func zoenComposerFittingHeight() -> NSNumber? { fittingHeight }
}

/** Hosted UIKit consumer-boundary tests; requires a foreground App scene.
 * The root is a native protocol port, not an ENRM rendering/gesture proof. */
@MainActor
final class ComposerSizingTests: XCTestCase {
  func testHeadingRunDoesNotMasqueradeAsBodyFontIdentity() throws {
    guard let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene })
      .first(where: { $0.activationState == .foregroundActive }) else {
      throw XCTSkip("Requires a foreground hosted UIWindowScene")
    }
    let window = UIWindow(windowScene: scene)
    let controller = UIViewController()
    window.rootViewController = controller
    window.makeKeyAndVisible()
    defer { window.isHidden = true }
    let root = CanonicalComposerRoot(frame: CGRect(x: 0, y: 0, width: 264, height: 80))
    root.accessibilityLabel = "Message"
    controller.view.addSubview(root)
    let editor = UITextView(frame: CGRect(x: 12, y: 8, width: 240, height: 64))
    editor.attributedText = NSAttributedString(string: "título", attributes: [.font: UIFont.systemFont(ofSize: 30)])
    root.addSubview(editor)
    root.fittingHeight = NSNumber(value: Double(editor.sizeThatFits(
      CGSize(width: 240, height: CGFloat.greatestFiniteMagnitude)).height))
    let measure = {
      ZoenComposerSizing.measure(root: root, tag: 1, expectedText: "# título", expectedLabel: "Message",
        expectedPlaceholder: "Message", width: 264, expectedFontSize: 17, nonce: 1)
    }
    let fitted = try XCTUnwrap(measure())
    let actual = editor.sizeThatFits(CGSize(width: 240, height: CGFloat.greatestFiniteMagnitude)).height + 16
    XCTAssertEqual(try XCTUnwrap(fitted["height"]), Double(actual), accuracy: 0.5)
    root.baseFontSize = 19
    XCTAssertNil(measure(), "A stale body-font request must still be rejected")
  }

  func testCanonicalEmptyFieldUsesRendererFitAndRetainsIdentityGuards() throws {
    guard let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene })
      .first(where: { $0.activationState == .foregroundActive }) else {
      throw XCTSkip("Requires a foreground hosted UIWindowScene")
    }
    let window = UIWindow(windowScene: scene)
    let controller = UIViewController()
    window.rootViewController = controller
    window.makeKeyAndVisible()
    defer { window.isHidden = true }
    let root = CanonicalComposerRoot(frame: CGRect(x: 0, y: 0, width: 264, height: 40))
    root.canonicalText = ""
    root.baseFontSize = 51
    root.fittingHeight = 61
    controller.view.addSubview(root)
    let editor = UITextView(frame: CGRect(x: 12, y: 9, width: 240, height: 21))
    // Fabric forwards the label to ENRM's accessibilityElement (UITextView).
    // Its non-accessible host has no label, and it never appends the placeholder.
    editor.accessibilityLabel = "Message"
    editor.textContainerInset = .zero
    editor.font = .systemFont(ofSize: 16)
    editor.typingAttributes = [.font: UIFont.systemFont(ofSize: 51)]
    root.addSubview(editor)
    let measure = {
      ZoenComposerSizing.measure(root: root, tag: 1, expectedText: "", expectedLabel: "Message",
        expectedPlaceholder: "Message", width: 264, expectedFontSize: 51, nonce: 2)
    }
    XCTAssertEqual(try XCTUnwrap(measure()?["height"]), 80, accuracy: 0.5)
    XCTAssertEqual(editor.text, "")
    XCTAssertEqual(editor.selectedRange, NSRange(location: 0, length: 0))
    editor.accessibilityLabel = "Message Message"
    XCTAssertNil(measure(), "The rich editor does not use RN's placeholder-suffixed label")
    editor.accessibilityLabel = "Another field"
    XCTAssertNil(measure(), "A stale accessibility identity must not measure another field")
    editor.accessibilityLabel = "Message"
    for height: NSNumber? in [nil, 0, -1, NSNumber(value: Double.nan), NSNumber(value: Double.infinity)] {
      root.fittingHeight = height
      XCTAssertNil(measure(), "Missing/invalid native geometry cannot fall back to the wrong font")
    }
    root.fittingHeight = 61
    root.canonicalText = "Another draft"
    XCTAssertNil(measure())
    root.canonicalText = ""
    root.baseFontSize = 17
    XCTAssertNil(measure())
  }
}
