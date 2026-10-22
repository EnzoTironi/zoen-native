import XCTest
import UIKit
@testable import ZoenCore

@MainActor extension AttachmentContainerTests {
  func testSendCarriesLastNativeKeystrokeBeforeReactEchoAndRetirementFencesRetainedButton() throws {
    var events: [(String, [String: Any])] = []
    let content = try content { events.append(($0, $1)) }
    let caption = try XCTUnwrap(find(content.view, type: UITextView.self, id: "attachment-caption"))
    let send = try XCTUnwrap(find(content.view, type: UIButton.self, id: "attachment-send"))
    caption.text = "last keystroke 🪁"
    // No delegate event or model echo: Send must read the actual native field.
    send.sendActions(for: .touchUpInside)
    XCTAssertEqual(events.map(\.0), ["send"])
    XCTAssertEqual(events.first?.1["text"] as? String, "last keystroke 🪁")
    content.retire()
    send.sendActions(for: .touchUpInside)
    content.textViewDidChange(caption)
    XCTAssertEqual(events.count, 1)
  }
  func testFileOptionCarriesFinalNativeCaptionAndOriginalSelectionWithoutRetargetingAStaleMenu() throws {
    var events: [(String, [String: Any])] = []
    let content = try content { events.append(($0, $1)) }
    var value = model
    value["revision"] = "selected-a"
    value["sendOptions"] = [["id": "send-as-file", "title": "Send as File", "enabled": true]]
    try content.update(model: value, appearance: appearance)
    let caption = try XCTUnwrap(find(content.view, type: UITextView.self, id: "attachment-caption"))
    let more = try XCTUnwrap(find(content.view, type: UIButton.self, id: "attachment-keyboard-review"))
    XCTAssertNil(more.menu, "Photos uses the pinned reference renderer rather than a UIKit menu")
    XCTAssertFalse(more.showsMenuAsPrimaryAction)
    caption.text = "final native file caption 🪁"
    content.performSendAsFileOption(revision: "selected-a", selection: ["ph://synthetic-a"])
    XCTAssertEqual(events.count, 1)
    XCTAssertEqual(events.first?.0, "option")
    XCTAssertEqual(events.first?.1["text"] as? String, caption.text)
    XCTAssertEqual(events.first?.1["ids"] as? [String], ["ph://synthetic-a"])
    XCTAssertEqual(events.first?.1["revision"] as? String, "selected-a")
    value["revision"] = "changed-selection"
    try content.update(model: value, appearance: appearance)
    content.performSendAsFileOption(revision: "selected-a", selection: ["ph://synthetic-a"])
    content.performSendAsFileOption(revision: "changed-selection", selection: ["ph://other"])
    XCTAssertEqual(events.count, 1)
    content.retire()
    content.performSendAsFileOption(revision: "changed-selection", selection: ["ph://synthetic-a"])
    XCTAssertEqual(events.count, 1)
  }
  func testFileOptionCannotBypassReconciliationDisabledModeOrCaptionBounds() throws {
    var events = 0
    let content = try content { _, _ in events += 1 }
    var value = model
    value["revision"] = "current"
    value["sendOptions"] = [["id": "send-as-file", "title": "Send as File", "enabled": false]]
    try content.update(model: value, appearance: appearance)
    content.performSendAsFileOption(revision: "current", selection: ["ph://synthetic-a"])
    value["sendOptions"] = [["id": "send-as-file", "title": "Send as File", "enabled": true]]
    try content.update(model: value, appearance: appearance)
    let caption = try XCTUnwrap(find(content.view, type: UITextView.self, id: "attachment-caption"))
    for text in [String(repeating: "🪁", count: 2049), "invalid\0caption"] {
      caption.text = text
      content.performSendAsFileOption(revision: "current", selection: ["ph://synthetic-a"])
    }
    XCTAssertEqual(events, 0)
    content.retire()
  }
  func testUngroupedOptionUsesExactSelectionRevisionAndFinalCaptionWithoutChangingTheSelection() throws {
    var events: [(String, [String: Any])] = []
    let content = try content { events.append(($0, $1)) }
    var value = model
    value["revision"] = "album-selection"
    value["items"] = [["id": "ph://synthetic-a", "name": "a.jpg", "type": "image"],
                      ["id": "ph://synthetic-b", "name": "b.jpg", "type": "image"]]
    value["selected"] = ["ph://synthetic-b", "ph://synthetic-a"]
    value["sendOptions"] = [["id": "send-without-grouping", "title": "Send Without Grouping", "enabled": true]]
    try content.update(model: value, appearance: appearance)
    let caption = try XCTUnwrap(find(content.view, type: UITextView.self, id: "attachment-caption"))
    caption.text = "last ungrouped keystroke"
    content.performSendOption(id: "send-without-grouping", revision: "stale", selection: ["ph://synthetic-b", "ph://synthetic-a"])
    content.performSendOption(id: "send-without-grouping", revision: "album-selection", selection: ["ph://synthetic-a", "ph://synthetic-b"])
    content.performSendOption(id: "unknown", revision: "album-selection", selection: ["ph://synthetic-b", "ph://synthetic-a"])
    XCTAssertTrue(events.isEmpty)
    content.performSendOption(id: "send-without-grouping", revision: "album-selection", selection: ["ph://synthetic-b", "ph://synthetic-a"])
    XCTAssertEqual(events.count, 1)
    XCTAssertEqual(events[0].0, "option")
    XCTAssertEqual(events[0].1["id"] as? String, "send-without-grouping")
    XCTAssertEqual(events[0].1["ids"] as? [String], ["ph://synthetic-b", "ph://synthetic-a"])
    XCTAssertEqual(events[0].1["text"] as? String, "last ungrouped keystroke")
    content.retire()
    content.performSendOption(id: "send-without-grouping", revision: "album-selection", selection: ["ph://synthetic-b", "ph://synthetic-a"])
    XCTAssertEqual(events.count, 1)
  }
  func testCaptionBoundsUseUTF16AndRejectNulWithoutPublishing() throws {
    var events = 0
    let content = try content { _, _ in events += 1 }
    let caption = try XCTUnwrap(find(content.view, type: UITextView.self, id: "attachment-caption"))
    let send = try XCTUnwrap(find(content.view, type: UIButton.self, id: "attachment-send"))
    caption.text = String(repeating: "🪁", count: 2048)
    XCTAssertTrue(content.textView(caption, shouldChangeTextIn: NSRange(location: 0, length: 0), replacementText: ""))
    XCTAssertFalse(content.textView(caption, shouldChangeTextIn: NSRange(location: 0, length: 0), replacementText: "x"))
    XCTAssertFalse(content.textView(caption, shouldChangeTextIn: NSRange(location: 0, length: 0), replacementText: "\0"))
    caption.text += "x"
    content.textViewDidChange(caption); send.sendActions(for: .touchUpInside)
    XCTAssertEqual(events, 0)
    var invalid = model
    invalid["caption"] = ["value": "not\0valid"]
    XCTAssertThrowsError(try content.update(model: invalid, appearance: appearance))
    content.retire()
  }
  func testAlbumAndCameraAvailabilityDoNotReserveAnUnavailableCameraCell() throws {
    let content = try content { _, _ in }
    let grid = content.attachmentScrollView as! UICollectionView
    XCTAssertEqual(content.collectionView(grid, numberOfItemsInSection: 0), 1)
    var current = model
    current["cameraAvailable"] = true
    try content.update(model: current, appearance: appearance)
    XCTAssertEqual(content.collectionView(grid, numberOfItemsInSection: 0), 2)
    current["albumId"] = "synthetic-album"
    try content.update(model: current, appearance: appearance)
    XCTAssertEqual(content.collectionView(grid, numberOfItemsInSection: 0), 1)
    content.retire()
    XCTAssertEqual(content.collectionView(grid, numberOfItemsInSection: 0), 1, "pixels remain while closing; action authority is already retired")
    XCTAssertThrowsError(try content.update(model: current, appearance: appearance))
  }

  func testSelectedCaptionJsonModelIsEditableAndReceivesSettledWindowHitTesting() throws {
    let data = try JSONSerialization.data(withJSONObject: model)
    let decoded = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    let content = ZoenAttachmentKeyboardContent { _, _ in }
    try content.update(model: decoded, appearance: appearance)
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
    window.rootViewController = UIViewController()
    window.isHidden = false
    let overlay = ZoenAttachmentContainer(content: content, appearance: appearance, requestClose: {})
    overlay.present(in: window)
    defer { content.retire(); overlay.dismiss(animated: false, completion: {}); window.isHidden = true }
    // This test inspects settled hit testing, not the animation's presentation.
    for view in [overlay] + overlay.subviews { view.layer.removeAllAnimations() }
    overlay.layoutIfNeeded(); content.view.layoutIfNeeded()
    let caption = try XCTUnwrap(find(content.view, type: UITextView.self, id: "attachment-caption"))
    XCTAssertTrue(caption.isEditable)
    XCTAssertTrue(caption.isSelectable)
    XCTAssertTrue(caption.isUserInteractionEnabled)
    XCTAssertTrue(caption.canBecomeFirstResponder)
    let point = caption.convert(CGPoint(x: caption.bounds.midX, y: caption.bounds.midY), to: window)
    let hit = try XCTUnwrap(window.hitTest(point, with: nil))
    XCTAssertTrue(hit === caption || hit.isDescendant(of: caption), "caption point instead hit \(type(of: hit)); frame \(hit.frame)")
  }


  func testFileTabRetainsPickerAndOnlyExplicitBrowseRequestsExternalHandoff() throws {
    var events: [(String, [String: Any])] = []
    let content = ZoenAttachmentKeyboardContent { events.append(($0, $1)) }
    content.view.frame = CGRect(x: 0, y: 0, width: 402, height: 578)
    var current = model
    current["selected"] = [String]()
    try content.update(model: current, appearance: appearance)
    content.updatePresentation(expansion: 0, keyboardHeight: 0, safeArea: .zero)
    content.view.layoutIfNeeded()
    let tab = try XCTUnwrap(find(content.view, type: UIButton.self, id: "attachment-keyboard-document"))
    let tabs = try XCTUnwrap(find(content.view, type: UIScrollView.self, id: "attachment-category-scroll"))
    let point = tab.convert(CGPoint(x: tab.bounds.midX, y: tab.bounds.midY), to: content.view)
    XCTAssertTrue(content.view.hitTest(point, with: nil) === tabs,
      "the original lens disables child interaction; its sibling scroll surface must receive real touches")
    XCTAssertTrue(tabs.gestureRecognizers?.contains { $0 is TabSelectionRecognizer } == true)
    tab.sendActions(for: .touchUpInside)
    XCTAssertEqual(events.map(\.0), ["category"])
    XCTAssertEqual(events.first?.1["id"] as? String, "document")
    current["activeCategory"] = "document"
    try content.update(model: current, appearance: appearance)
    content.view.layoutIfNeeded()
    XCTAssertEqual(content.attachmentScrollView.accessibilityIdentifier, "attachment-file-list")
    let browse = try XCTUnwrap(find(content.view, type: UIButton.self, id: "attachment-file-browse"))
    browse.sendActions(for: .touchUpInside)
    XCTAssertEqual(events.map(\.0), ["category", "document"])
    content.retire()
    browse.sendActions(for: .touchUpInside)
    XCTAssertEqual(events.count, 2)
  }
}
