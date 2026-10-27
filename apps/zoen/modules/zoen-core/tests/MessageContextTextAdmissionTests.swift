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
  private func githubRenderer(_ markdown: String) throws -> (UIView, UIView, [UITextView]) {
    let type = try XCTUnwrap(NSClassFromString("EnrichedMarkdown") as? UIView.Type)
    let internalType: AnyClass = try XCTUnwrap(NSClassFromString("EnrichedMarkdownInternalText"))
    let renderer = type.init(frame: CGRect(x: 0, y: 0, width: 300, height: 800))
    let render = NSSelectorFromString("renderMarkdownSynchronously:")
    XCTAssertTrue(renderer.responds(to: render), "use the real GitHub-flavor public parser entrypoint")
    renderer.perform(render, with: markdown as NSString)
    let boundary = UIView(frame: renderer.bounds)
    boundary.addSubview(renderer)
    var bodies: [UITextView] = []
    func visit(_ view: UIView) {
      if view.isKind(of: internalType), let text = view.subviews.compactMap({ $0 as? UITextView }).first {
        // Standalone construction has no Fabric layout metrics. These fixture
        // frames qualify native policy/attribution, not application geometry.
        view.frame = CGRect(x: 0, y: CGFloat(bodies.count) * 140, width: 300, height: 120)
        text.frame = view.bounds
        text.isHidden = false; text.isEditable = false; text.isSelectable = false
        bodies.append(text)
      }
      for child in view.subviews { visit(child) }
    }
    visit(renderer)
    XCTAssertFalse(bodies.isEmpty, "actual parser must construct InternalText segments")
    return (boundary, renderer, bodies)
  }

  private func glyphPoint(_ text: UITextView, range: NSRange) -> CGPoint {
    text.layoutManager.ensureLayout(for: text.textContainer)
    let glyphs = text.layoutManager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
    let rect = text.layoutManager.boundingRect(forGlyphRange: glyphs, in: text.textContainer)
    return CGPoint(x: rect.midX + text.textContainerInset.left, y: rect.midY + text.textContainerInset.top)
  }

  func testLinkedGithubRendererApprovesOriginalBodyButRetainsItsActualLink() throws {
    let (boundary, _, bodies) = try githubRenderer("Original **body** [link](https://example.invalid)")
    let text = try XCTUnwrap(bodies.first)
    let body = (text.text as NSString).range(of: "body")
    let link = (text.text as NSString).range(of: "link")
    XCTAssertNotEqual(body.location, NSNotFound)
    XCTAssertNotEqual(link.location, NSNotFound)
    XCTAssertTrue(ZoenMessageContextTextAdmission.allows(text, point: glyphPoint(text, range: body), boundary: boundary))
    XCTAssertFalse(ZoenMessageContextTextAdmission.allows(text, point: glyphPoint(text, range: link), boundary: boundary))
  }

  func testGithubSecondParagraphAfterCodeKeepsOriginalUtf16AnchorForDuplicateUnicodeText() throws {
    let source = "😀 same\n\n```\nblocked\n```\n\n😀 **same** second"
    let (boundary, _, bodies) = try githubRenderer(source)
    let matches = bodies.filter { $0.text.contains("same") }
    XCTAssertEqual(matches.count, 2, "code boundary produces distinct original text segments")
    let second = try XCTUnwrap(matches.last)
    let selected = (second.text as NSString).range(of: "same")
    XCTAssertNotEqual(selected.location, NSNotFound)
    XCTAssertTrue(ZoenMessageContextTextAdmission.allows(second, point: glyphPoint(second, range: selected), boundary: boundary))
    var whole = NSRange()
    let attribute = second.attributedText.attribute(NSAttributedString.Key("ENRMQuoteSource"),
      at: selected.location, longestEffectiveRange: &whole,
      in: NSRange(location: 0, length: second.attributedText.length))
    let anchor = try XCTUnwrap(attribute as? [String: Any])
    let start = try XCTUnwrap(anchor["start"] as? NSNumber).intValue
    let original = start + selected.location - whole.location
    XCTAssertEqual(original, 31, "global UTF-16 offset includes prior astral scalar, paragraph and code block")
    XCTAssertEqual((source as NSString).substring(with: NSRange(location: original, length: selected.length)), "same")
    XCTAssertEqual((source as NSString).substring(with: NSRange(location: 3, length: 4)), "same")
    XCTAssertNotEqual(original, 3, "duplicate text cannot resolve to the first paragraph")
  }

  func testOriginalHeldSecondViewAndSecondParagraphDoNotRetargetFirstDuplicateText() throws {
    let first = UITextView(frame: CGRect(x: 0, y: 0, width: 300, height: 100))
    let held = UITextView(frame: first.frame)
    first.text = "same"
    held.text = "😀 same\nsecond same\n"
    for text in [first, held] { text.font = .systemFont(ofSize: 17); text.isEditable = false }
    let second = (held.text as NSString).range(of: "second")
    let point = glyphPoint(held, range: NSRange(location: second.location, length: 1))
    let target = try XCTUnwrap(ZoenMessageHeldParagraph.target(in: [first, held], held: held, hadHeld: true))
    XCTAssertTrue(target === held)
    let range = try XCTUnwrap(ZoenMessageHeldParagraph.range(in: target, point: point))
    XCTAssertEqual(range, NSRange(location: 8, length: 11))
    XCTAssertEqual((target.text as NSString).substring(with: range), "second same")
    XCTAssertFalse((target.text as NSString).substring(with: range).contains("\n"))
  }

  func testRetiredHeldLeafCannotFallBackToAnotherViewAndPlainFallbackMustBeUnambiguous() {
    let replacement = UITextView()
    replacement.text = "Other body"
    let retired = UITextView()
    XCTAssertNil(ZoenMessageHeldParagraph.target(in: [replacement], held: retired, hadHeld: true))
    XCTAssertNil(ZoenMessageHeldParagraph.target(in: [replacement], held: nil, hadHeld: true))
    XCTAssertNil(ZoenMessageHeldParagraph.target(in: [replacement, retired], held: nil, hadHeld: false))
    XCTAssertTrue(ZoenMessageHeldParagraph.target(in: [replacement], held: nil, hadHeld: false) === replacement)
    XCTAssertEqual(ZoenMessageHeldParagraph.range(in: replacement, point: nil), NSRange(location: 0, length: 10))
    replacement.text = "First\nSecond"
    XCTAssertNil(ZoenMessageHeldParagraph.range(in: replacement, point: nil), "missing original point cannot choose an arbitrary paragraph")
    replacement.text = ""
    XCTAssertNil(ZoenMessageHeldParagraph.range(in: replacement, point: nil))
  }

  func testAdmittedGestureTransfersOriginalTextPointThroughSourcePresentation() async throws {
    let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first { $0.activationState == .foregroundActive })
    let window = UIWindow(windowScene: scene)
    window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
    let parent = UIViewController(); window.rootViewController = parent; window.makeKeyAndVisible()
    let stage = ZoenMessageContextStage(); stage.frame = parent.view.bounds; parent.view.addSubview(stage)
    let source = ZoenMessageContextSource(); source.frame = CGRect(x: 20, y: 160, width: 240, height: 240)
    let child = UIView(frame: source.bounds)
    let first = UITextView(frame: CGRect(x: 0, y: 0, width: 240, height: 100))
    let held = UITextView(frame: CGRect(x: 0, y: 110, width: 240, height: 120))
    first.text = "same"; held.text = "😀 same\nsecond same"
    first.isEditable = false; held.isEditable = false
    child.addSubview(first); child.addSubview(held)
    stage.mountChildComponentView(source, index: 0); source.mountChildComponentView(child, index: 0)
    let binding = UUID().uuidString, request = UUID().uuidString
    source.bind(binding, active: true, handlesTap: true)
    stage.layoutIfNeeded(); source.layoutIfNeeded()
    var gesture: String?
    source.onGesture.onEventSent = { event in if event["kind"] as? String == "hold" { gesture = event["gestureId"] as? String } }
    source.dispatch(kind: "hold", textView: held, textPoint: glyphPoint(held, range: NSRange(location: 8, length: 1)))
    let gestureId = try XCTUnwrap(gesture)
    let theme: [String: Any] = ["background": "#FFFFFF", "highlight": "#EEEEEE", "text": "#000000", "secondary": "#777777", "separator": "#AAAAAA", "destructive": "#FF0000", "dim": "#00000080", "isDark": false]
    let data = try JSONSerialization.data(withJSONObject: ["actions": [["id": "select", "label": "Select"]], "reactions": [], "theme": theme, "reducedMotion": true, "incoming": true])
    let task = Task { try await source.present(requestId: request, expectedBinding: binding, gestureId: gestureId, configuration: String(decoding: data, as: UTF8.self)) }
    await Task.yield()
    let controller = try XCTUnwrap(stage.controller)
    XCTAssertTrue(controller.heldText === held)
    controller.choose("select")
    XCTAssertEqual(held.selectedRange, NSRange(location: 8, length: 11))
    XCTAssertFalse(first.isFirstResponder)
    await source.dismiss(requestId: request)
    let result = try await task.value
    XCTAssertNil(result)
    XCTAssertTrue(source.contentHost.superview === source)
  }
  func testLostOriginalHeldLeafBeforeDispatchCannotRetargetExistingText() async throws {
    let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first { $0.activationState == .foregroundActive })
    let window = UIWindow(windowScene: scene)
    window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
    let parent = UIViewController(); window.rootViewController = parent; window.makeKeyAndVisible()
    let stage = ZoenMessageContextStage(); stage.frame = parent.view.bounds; parent.view.addSubview(stage)
    let source = ZoenMessageContextSource(); source.frame = CGRect(x: 20, y: 160, width: 240, height: 240)
    let child = UIView(frame: source.bounds)
    let first = UITextView(frame: CGRect(x: 0, y: 0, width: 240, height: 100))
    let held = UITextView(frame: CGRect(x: 0, y: 110, width: 240, height: 120))
    first.text = "same"; held.text = "😀 same\nsecond same"
    first.isEditable = false; held.isEditable = false
    child.addSubview(first); child.addSubview(held)
    stage.mountChildComponentView(source, index: 0); source.mountChildComponentView(child, index: 0)
    let binding = UUID().uuidString, request = UUID().uuidString
    source.bind(binding, active: true, handlesTap: true)
    stage.layoutIfNeeded(); source.layoutIfNeeded()
    var gesture: String?
    source.onGesture.onEventSent = { event in if event["kind"] as? String == "hold" { gesture = event["gestureId"] as? String } }
    source.dispatch(kind: "hold", textView: nil, textPoint: glyphPoint(held, range: NSRange(location: 8, length: 1)), hadText: true)
    let gestureId = try XCTUnwrap(gesture)
    let theme: [String: Any] = ["background": "#FFFFFF", "highlight": "#EEEEEE", "text": "#000000", "secondary": "#777777", "separator": "#AAAAAA", "destructive": "#FF0000", "dim": "#00000080", "isDark": false]
    let data = try JSONSerialization.data(withJSONObject: ["actions": [["id": "select", "label": "Select"]], "reactions": [], "theme": theme, "reducedMotion": true, "incoming": true])
    let task = Task { try await source.present(requestId: request, expectedBinding: binding, gestureId: gestureId, configuration: String(decoding: data, as: UTF8.self)) }
    await Task.yield()
    let controller = try XCTUnwrap(stage.controller)
    XCTAssertNil(controller.heldText)
    XCTAssertTrue(controller.hadHeldText)
    controller.choose("select")
    XCTAssertFalse(controller.textSelectionInitialized)
    XCTAssertFalse(held.isFirstResponder)
    XCTAssertFalse(first.isFirstResponder)
    await source.dismiss(requestId: request)
    let result = try await task.value
    XCTAssertNil(result)
    XCTAssertTrue(source.contentHost.superview === source)
  }

}
