import UIKit
import XCTest
@testable import ZoenCore

@MainActor
private final class CanonicalComposerRoot: UIView {
  var baseFontSize = 17.0
  @objc func zoenComposerCanonicalText() -> String { "# título" }
  @objc func zoenComposerBaseFontSize() -> NSNumber { NSNumber(value: baseFontSize) }
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
}
