import XCTest
import UIKit
@testable import ZoenCore

// AUTHORED, NOT_RUN: hosted UIKit XCTest target. Renderer semantic hit-testing
// is qualified separately against the actual maintained EnrichedMarkdown patch.
@MainActor
final class MessageContextTextAdmissionTests: XCTestCase {
  private final class Renderer: UIView {
    var accepts = true
    var received: CGPoint?
    @objc func zoenMessageContextBodyAtPoint(_ value: NSValue) -> NSNumber {
      received = value.cgPointValue
      return NSNumber(value: accepts)
    }
  }
  private func fixture() -> (UIView, Renderer, UITextView) {
    let boundary = UIView(frame: CGRect(x: 0, y: 0, width: 300, height: 200))
    let renderer = Renderer(frame: CGRect(x: 20, y: 30, width: 220, height: 100))
    let text = UITextView(frame: CGRect(x: 10, y: 15, width: 180, height: 60))
    text.text = "Actual original body"
    text.isEditable = false; text.isSelectable = false
    boundary.addSubview(renderer); renderer.addSubview(text)
    return (boundary, renderer, text)
  }
  func testOnlyRendererApprovedReadOnlyNonselectableBodyIsAdmitted() {
    let (boundary, renderer, text) = fixture()
    XCTAssertTrue(ZoenMessageContextTextAdmission.allows(text, point: CGPoint(x: 5, y: 7), boundary: boundary))
    XCTAssertEqual(renderer.received, CGPoint(x: 15, y: 22))
    text.isSelectable = true
    XCTAssertFalse(ZoenMessageContextTextAdmission.allows(text, point: .zero, boundary: boundary))
    text.isSelectable = false; text.isEditable = true
    XCTAssertFalse(ZoenMessageContextTextAdmission.allows(text, point: .zero, boundary: boundary))
  }
  func testRendererOwnedInteractivePointKeepsItsNativeInput() {
    let (boundary, renderer, text) = fixture()
    // Renderer returns false for its actual links/code/spoilers/tasks/images.
    renderer.accepts = false
    XCTAssertFalse(ZoenMessageContextTextAdmission.allows(text, point: CGPoint(x: 5, y: 7), boundary: boundary))
    XCTAssertNotNil(renderer.received)
  }
  func testUnknownRendererForeignTextAndOutOfBoundsFailClosed() {
    let (boundary, renderer, text) = fixture()
    XCTAssertFalse(ZoenMessageContextTextAdmission.allows(text, point: CGPoint(x: -1, y: 0), boundary: boundary))
    XCTAssertFalse(ZoenMessageContextTextAdmission.allows(text, point: CGPoint(x: CGFloat.infinity, y: 0), boundary: boundary))
    XCTAssertNil(renderer.received)
    XCTAssertFalse(ZoenMessageContextTextAdmission.allows(text, point: .zero, boundary: UIView()))
    text.removeFromSuperview(); boundary.addSubview(text)
    XCTAssertFalse(ZoenMessageContextTextAdmission.allows(text, point: .zero, boundary: boundary))
  }
  func testNearestRendererRefusalCannotFallThroughToOuterProvider() {
    let (boundary, outer, text) = fixture()
    let inner = Renderer(frame: outer.bounds)
    inner.accepts = false
    text.removeFromSuperview(); outer.addSubview(inner); inner.addSubview(text)
    XCTAssertFalse(ZoenMessageContextTextAdmission.allows(text, point: CGPoint(x: 5, y: 7), boundary: boundary))
    XCTAssertNotNil(inner.received)
    XCTAssertNil(outer.received)
  }
  func testLinkedEnrichedMarkdownRendererApprovesBodyButRetainsItsOwnInteractiveText() throws {
    // Requires the hosted App target to link the actual maintained renderer;
    // this fixture does not replace its point mapping or interactive policy.
    let type = try XCTUnwrap(NSClassFromString("EnrichedMarkdownText") as? UIView.Type)
    let renderer = type.init(frame: CGRect(x: 0, y: 0, width: 240, height: 100))
    let text = try XCTUnwrap(renderer.subviews.compactMap { $0 as? UITextView }.first)
    let boundary = UIView(frame: renderer.bounds)
    boundary.addSubview(renderer)
    text.frame = renderer.bounds
    text.isHidden = false; text.isEditable = false; text.isSelectable = false
    text.textContainerInset = .zero
    text.textContainer.lineFragmentPadding = 0
    for font in [UIFont.systemFont(ofSize: 17), UIFont.boldSystemFont(ofSize: 17)] {
      let body = NSMutableAttributedString(string: "Original body", attributes: [.font: font])
      text.attributedText = body
      text.layoutManager.ensureLayout(for: text.textContainer)
      let rect = text.layoutManager.boundingRect(forGlyphRange: NSRange(location: 0, length: 1), in: text.textContainer)
      let point = CGPoint(x: rect.midX, y: rect.midY)
      XCTAssertTrue(ZoenMessageContextTextAdmission.allows(text, point: point, boundary: boundary))
      for attribute in ["linkURL", "codeBlockText"] {
        let interactive = NSMutableAttributedString(attributedString: body)
        interactive.addAttribute(NSAttributedString.Key(attribute), value: "owned", range: NSRange(location: 0, length: body.length))
        text.attributedText = interactive
        text.layoutManager.ensureLayout(for: text.textContainer)
        XCTAssertFalse(ZoenMessageContextTextAdmission.allows(text, point: point, boundary: boundary))
      }
    }
  }

}
