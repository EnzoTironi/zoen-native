import XCTest
import UIKit
@testable import ZoenCore

/// Actual production UIKit/glass/menu owner. These authored tests require an
/// iOS runner; SDK typechecking alone is not evidence that they executed.
@MainActor final class AttachmentReferenceMenuTests: XCTestCase {
  private let appearance = ZoenKeyboardPanelAppearance(background: .white, text: .black,
    secondary: .gray, accent: .blue, separator: .lightGray, bottomInset: 34)
  private func find<T: UIView>(_ view: UIView, type: T.Type, id: String) -> T? {
    if let result = view as? T, result.accessibilityIdentifier == id { return result }
    for child in view.subviews { if let result = find(child, type: type, id: id) { return result } }
    return nil
  }
  private func fixture() -> (UIWindow, UIViewController, UIButton) {
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
    let parent = UIViewController(); window.rootViewController = parent
    let button = UIButton(type: .system); button.frame = CGRect(x: 342, y: 80, width: 44, height: 44)
    parent.view.addSubview(button); window.isHidden = false
    return (window, parent, button)
  }
  func testActualActionDismissesOnceAndRetainedControlCannotSendAfterInvalidation() throws {
    let (window, parent, source) = fixture(); defer { window.isHidden = true }
    var valid = true, sends = 0, removed = 0
    let menu = ZoenAttachmentReferenceMenu(source: source, actions: [
      .init(id: "send-as-file", title: "Send as File", image: nil, enabled: true, perform: { sends += 1 })],
      dismissLabel: "Close", appearance: appearance, isValid: { valid }, reducedMotion: { true }, didDismiss: { _ in removed += 1 })
    XCTAssertTrue(menu.present(in: window)); XCTAssertEqual(parent.children.count, 1)
    let row = try XCTUnwrap(find(menu, type: UIControl.self, id: "attachment-reference-action-send-as-file"))
    XCTAssertEqual(row.accessibilityLabel, "Send as File")
    let point = row.convert(CGPoint(x: row.bounds.midX, y: row.bounds.midY), to: menu)
    XCTAssertTrue(menu.hitTest(point, with: nil) === row)
    row.sendActions(for: .touchUpInside); row.sendActions(for: .touchUpInside)
    XCTAssertEqual(sends, 1, "dismissal must fence a retained touch callback immediately")
    valid = false; menu.retire(); row.sendActions(for: .touchUpInside)
    XCTAssertEqual(sends, 1); XCTAssertEqual(removed, 1); XCTAssertTrue(parent.children.isEmpty)
    XCTAssertFalse(source.accessibilityElementsHidden)
  }
  func testOutsideCancelDisabledActionAndParentLossRestoreOwnershipWithoutSending() throws {
    let (window, parent, source) = fixture(); defer { window.isHidden = true }
    var sends = 0, removals = 0
    for invalidation in 0..<3 {
      let menu = ZoenAttachmentReferenceMenu(source: source, actions: [
        .init(id: "disabled", title: "Unavailable", image: nil, enabled: false, perform: { sends += 1 })],
        dismissLabel: "Close", appearance: appearance, isValid: { true }, reducedMotion: { false }, didDismiss: { _ in removals += 1 })
      XCTAssertTrue(menu.present(in: window))
      let row = try XCTUnwrap(find(menu, type: UIControl.self, id: "attachment-reference-action-disabled"))
      row.sendActions(for: .touchUpInside); menu.choose(0)
      XCTAssertEqual(sends, 0)
      if invalidation == 0 {
        let outside = try XCTUnwrap(menu.hitTest(CGPoint(x: 20, y: 500), with: nil) as? UIControl)
        // A clear hit target also needs rendered contents for iOS 27 delivery.
        // This structural assertion and sendActions do not prove real OS taps.
        let contents = try XCTUnwrap(outside.layer.contents)
        guard CFGetTypeID(contents as CFTypeRef) == CGImage.typeID else {
          XCTFail("outside-dismiss contents must be an actual rendered image"); return
        }
        let rendered = contents as! CGImage
        XCTAssertEqual(rendered.width, 1); XCTAssertEqual(rendered.height, 1)
        outside.sendActions(for: .touchUpInside)
      } else if invalidation == 1 { parent.view.removeFromSuperview() }
      menu.retire(); row.sendActions(for: .touchUpInside)
      XCTAssertEqual(sends, 0); XCTAssertTrue(parent.children.isEmpty)
      if invalidation == 1 { window.addSubview(parent.view) }
    }
    XCTAssertEqual(removals, 3); XCTAssertFalse(source.accessibilityElementsHidden)
  }
  func testPhotosMenuReadsActualFinalCaptionAndRejectsRetainedRowAfterRevisionChange() throws {
    let (window, parent, _) = fixture(); defer { window.isHidden = true }
    var events: [(String, [String: Any])] = []
    let content = ZoenAttachmentKeyboardContent { events.append(($0, $1)) }
    var model: [String: Any] = ["items": [["id": "ph://a", "name": "a.jpg", "type": "image"]],
      "selected": ["ph://a"], "labels": ["close": "Close", "more": "More"],
      "revision": "first", "sendOptions": [["id": "send-as-file", "title": "Send as File", "enabled": true]],
      "albumId": NSNull(), "granted": true, "cameraAvailable": false,
      "caption": ["value": "", "placeholder": "Caption", "editable": true, "sending": false, "count": 1, "sendLabel": "Send"]]
    content.view.frame = window.bounds; parent.view.addSubview(content.view)
    try content.update(model: model, appearance: appearance)
    content.view.layoutIfNeeded()
    let caption = try XCTUnwrap(find(content.view, type: UITextView.self, id: "attachment-caption"))
    let more = try XCTUnwrap(find(content.view, type: UIButton.self, id: "attachment-keyboard-review"))
    more.sendActions(for: .touchUpInside)
    let firstMenu = try XCTUnwrap(find(parent.view, type: ZoenAttachmentReferenceMenu.self, id: "attachment-reference-menu"))
    let retained = try XCTUnwrap(find(firstMenu, type: UIControl.self, id: "attachment-reference-action-send-as-file"))
    model["revision"] = "second"
    try content.update(model: model, appearance: appearance)
    XCTAssertNil(firstMenu.superview)
    retained.sendActions(for: .touchUpInside); XCTAssertTrue(events.isEmpty)
    more.sendActions(for: .touchUpInside)
    let secondMenu = try XCTUnwrap(find(parent.view, type: ZoenAttachmentReferenceMenu.self, id: "attachment-reference-menu"))
    let action = try XCTUnwrap(find(secondMenu, type: UIControl.self, id: "attachment-reference-action-send-as-file"))
    caption.text = "last native keystroke 🪁"
    action.sendActions(for: .touchUpInside); action.sendActions(for: .touchUpInside)
    XCTAssertEqual(events.map(\.0), ["option"])
    XCTAssertEqual(events.first?.1["revision"] as? String, "second")
    XCTAssertEqual(events.first?.1["ids"] as? [String], ["ph://a"])
    XCTAssertEqual(events.first?.1["text"] as? String, caption.text)
    XCTAssertTrue(find(content.view, type: UITextView.self, id: "attachment-caption") === caption)
    content.retire(); XCTAssertNil(secondMenu.superview)
  }
}
