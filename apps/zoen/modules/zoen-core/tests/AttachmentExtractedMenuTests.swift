import XCTest
import UIKit
@testable import ZoenCore

/// These exercise real production UIKit ownership/callbacks when run on iOS.
/// Calling admission directly does not qualify physical long-press delivery.
@MainActor final class AttachmentExtractedMenuTests: XCTestCase {
  private let appearance = ZoenKeyboardPanelAppearance(background: .white, text: .black,
    secondary: .gray, accent: .blue, separator: .lightGray, bottomInset: 34)
  private func find<T: UIView>(_ root: UIView, type: T.Type, id: String) -> T? {
    if let view = root as? T, view.accessibilityIdentifier == id { return view }
    for child in root.subviews { if let result = find(child, type: type, id: id) { return result } }
    return nil
  }
  func testRealContentReturnsOnceOnInterruptedDismissAndParentLoss() throws {
    for parentLoss in [false, true] {
      let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
      let parent = ZoenPresentationRootController(); window.rootViewController = parent
      let anchor = UIView(frame: CGRect(x: 16, y: 700, width: 370, height: 60))
      let body = UIView(frame: anchor.bounds), label = UILabel()
      body.addSubview(label); anchor.addSubview(body); parent.view.addSubview(anchor); window.isHidden = false
      defer { window.isHidden = true }
      var sends = 0, removals = 0
      let content = ZoenAttachmentExtractedContent(anchor: anchor, content: body, updateExtracted: { _, _ in })
      let menu = ZoenAttachmentExtractedMenu(source: content, actions: [
        .init(id: "select", title: "Select", image: nil, enabled: true, perform: { sends += 1 })],
        dismissLabel: "Close", appearance: appearance, isValid: { true }, didDismiss: { _ in removals += 1 })
      XCTAssertTrue(menu.present(in: window)); XCTAssertFalse(body.superview === anchor)
      XCTAssertTrue(label.superview === body); XCTAssertFalse(body.isUserInteractionEnabled)
      if parentLoss { parent.view.removeFromSuperview() } else { menu.dismiss(animated: true) }
      menu.retire(); menu.choose(0)
      XCTAssertTrue(body.superview === anchor); XCTAssertTrue(label.superview === body)
      XCTAssertEqual(body.frame, anchor.bounds); XCTAssertTrue(body.isUserInteractionEnabled)
      XCTAssertTrue(parent.children.isEmpty); XCTAssertEqual(removals, 1); XCTAssertEqual(sends, 0)
    }
  }
  func testPopulatedFileRowExtractionRestoresBeforeRevisionReconfigurationAndRetainedActionIsInert() async throws {
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
    let parent = ZoenPresentationRootController(); window.rootViewController = parent
    var events: [(String, [String: Any])] = []
    let content = ZoenAttachmentFileContent { events.append(($0, $1)) }
    parent.view.addSubview(content.view); content.view.frame = window.bounds; window.isHidden = false
    defer { content.retire(); window.isHidden = true }
    var model: [String: Any] = ["revision": "r1", "activeCategory": "document", "fileItems": [
      ["id": "canonical-source", "name": "Presentation.pdf", "mime": "application/pdf", "bytes": "51", "at_ms": "10"]],
      "recent": ["selected": [String](), "query": "", "searching": false, "loading": false, "hasMore": false],
      "labels": ["previewFile": "Preview", "selectFile": "Select", "close": "Close", "more": "More"]]
    try content.update(model: model, appearance: appearance); content.setVisible(true)
    content.updateLayout(navigationHeight: 56, bottomInset: 90, safeArea: .zero)
    content.scrollView.layoutIfNeeded()
    let index = IndexPath(row: 0, section: 1)
    let cell = try XCTUnwrap(content.scrollView.cellForRow(at: index))
    let anchor = try XCTUnwrap(find(cell, type: UIView.self, id: "attachment-file-extraction-source"))
    let body = try XCTUnwrap(find(anchor, type: UIView.self, id: "attachment-file-extraction-content"))
    let gesture = try XCTUnwrap(anchor.gestureRecognizers?.compactMap { $0 as? ZoenAttachmentContextGesture }.first)
    XCTAssertNil(content.tableView(content.scrollView, contextMenuConfigurationForRowAt: index, point: .zero))
    XCTAssertTrue(gesture.activated?(gesture, .zero) == true)
    let first = try XCTUnwrap(find(parent.view, type: ZoenAttachmentExtractedMenu.self, id: "attachment-extracted-menu"))
    let retained = try XCTUnwrap(find(first, type: UIControl.self, id: "attachment-reference-action-select"))
    XCTAssertFalse(body.superview === anchor); XCTAssertTrue(events.isEmpty)
    model["revision"] = "r2"; model["busy"] = true
    try content.update(model: model, appearance: appearance)
    XCTAssertTrue(body.superview === anchor); XCTAssertNil(first.superview)
    retained.sendActions(for: .touchUpInside); XCTAssertTrue(events.isEmpty)
    model["revision"] = "r3"; model["busy"] = false
    try content.update(model: model, appearance: appearance)
    XCTAssertTrue(gesture.activated?(gesture, .zero) == true)
    let second = try XCTUnwrap(find(parent.view, type: ZoenAttachmentExtractedMenu.self, id: "attachment-extracted-menu"))
    let preview = try XCTUnwrap(find(second, type: UIControl.self, id: "attachment-reference-action-preview"))
    preview.sendActions(for: .touchUpInside); preview.sendActions(for: .touchUpInside)
    XCTAssertTrue(events.isEmpty); XCTAssertFalse(body.superview === anchor)
    let returned = expectation(for: NSPredicate { _, _ in second.superview == nil }, evaluatedWith: NSNull())
    await fulfillment(of: [returned], timeout: 2)
    XCTAssertEqual(events.map(\.0), ["recentPreview"])
    XCTAssertEqual(events.first?.1["id"] as? String, "canonical-source")
    XCTAssertEqual(events.first?.1["revision"] as? String, "r3")
    content.setVisible(false); XCTAssertTrue(body.superview === anchor); XCTAssertNil(second.superview)
  }
  func testSelectActsImmediatelyButRevisionUpdateKeepsActualReturnAliveUntilRestoration() async throws {
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
    let parent = ZoenPresentationRootController(); window.rootViewController = parent
    var events: [(String, [String: Any])] = []
    let content = ZoenAttachmentFileContent { events.append(($0, $1)) }
    parent.view.addSubview(content.view); content.view.frame = window.bounds; window.isHidden = false
    defer { content.retire(); window.isHidden = true }
    var model: [String: Any] = ["revision": "r1", "activeCategory": "document", "fileItems": [
      ["id": "source", "name": "Presentation.pdf", "mime": "application/pdf", "bytes": "51"]],
      "recent": ["selected": [String](), "searching": false],
      "labels": ["previewFile": "Preview", "selectFile": "Select", "close": "Close", "more": "More"]]
    try content.update(model: model, appearance: appearance); content.setVisible(true)
    content.updateLayout(navigationHeight: 56, bottomInset: 90, safeArea: .zero); content.scrollView.layoutIfNeeded()
    let cell = try XCTUnwrap(content.scrollView.cellForRow(at: IndexPath(row: 0, section: 1)))
    let anchor = try XCTUnwrap(find(cell, type: UIView.self, id: "attachment-file-extraction-source"))
    let body = try XCTUnwrap(find(anchor, type: UIView.self, id: "attachment-file-extraction-content"))
    let gesture = try XCTUnwrap(anchor.gestureRecognizers?.compactMap { $0 as? ZoenAttachmentContextGesture }.first)
    XCTAssertTrue(gesture.activated?(gesture, .zero) == true)
    let menu = try XCTUnwrap(find(parent.view, type: ZoenAttachmentExtractedMenu.self, id: "attachment-extracted-menu"))
    let select = try XCTUnwrap(find(menu, type: UIControl.self, id: "attachment-reference-action-select"))
    select.sendActions(for: .touchUpInside)
    XCTAssertEqual(events.map(\.0), ["recentSelect"]); XCTAssertTrue(menu.isReturningAfterAction)
    model["revision"] = "r2"; model["recent"] = ["selected": ["source"], "searching": false]
    try content.update(model: model, appearance: appearance)
    XCTAssertNotNil(menu.superview); XCTAssertFalse(body.superview === anchor)
    select.sendActions(for: .touchUpInside); XCTAssertEqual(events.count, 1)
    let returned = expectation(for: NSPredicate { _, _ in menu.superview == nil }, evaluatedWith: NSNull())
    await fulfillment(of: [returned], timeout: 2)
    content.scrollView.layoutIfNeeded()
    let selectedCell = try XCTUnwrap(content.scrollView.cellForRow(at: IndexPath(row: 0, section: 1)))
    XCTAssertTrue(body.superview === anchor)
    XCTAssertTrue(selectedCell.accessibilityTraits.contains(.selected)); XCTAssertEqual(events.count, 1)
  }
  func testCanceledPreviewReturnNeverHandsOffAndFirstMoveAloneDoesNotSelect() throws {
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
    let parent = ZoenPresentationRootController(); window.rootViewController = parent
    let anchor = UIView(frame: CGRect(x: 16, y: 100, width: 370, height: 60))
    let body = UIView(frame: anchor.bounds); anchor.addSubview(body); parent.view.addSubview(anchor)
    window.isHidden = false; defer { window.isHidden = true }
    var actions = 0
    let menu = ZoenAttachmentExtractedMenu(source: .init(anchor: anchor, content: body, updateExtracted: { _, _ in }),
      actions: [.init(id: "preview", title: "Preview", image: nil, enabled: true, invocation: .afterContentReturns, perform: { actions += 1 })],
      dismissLabel: "Close", appearance: appearance, isValid: { true }, didDismiss: { _ in })
    XCTAssertTrue(menu.present(in: window))
    let row = try XCTUnwrap(find(menu, type: UIControl.self, id: "attachment-reference-action-preview"))
    let point = row.convert(CGPoint(x: row.bounds.midX, y: row.bounds.midY), to: parent.view)
    // The pin uses the first external move, not activation, as its baseline.
    menu.continueSourceGesture(view: parent.view, point: point); menu.endSourceGesture(performAction: true)
    XCTAssertEqual(actions, 0); XCTAssertFalse(menu.isReturningAfterAction)
    menu.continueSourceGesture(view: parent.view, point: point.applying(.init(translationX: 0, y: 5)))
    menu.endSourceGesture(performAction: false); XCTAssertEqual(actions, 0)
    menu.choose(0); XCTAssertEqual(actions, 0)
    menu.retire(); XCTAssertTrue(body.superview === anchor); XCTAssertEqual(actions, 0)
  }
  func testChangedSourceSizeInterruptsAndRestoresIntoLiveRowBounds() throws {
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
    let parent = ZoenPresentationRootController(); window.rootViewController = parent
    let anchor = UIView(frame: CGRect(x: 16, y: 100, width: 370, height: 60))
    let body = UIView(frame: anchor.bounds); anchor.addSubview(body); parent.view.addSubview(anchor)
    window.isHidden = false; defer { window.isHidden = true }
    var actions = 0
    let menu = ZoenAttachmentExtractedMenu(source: .init(anchor: anchor, content: body, updateExtracted: { _, _ in }),
      actions: [.init(id: "select", title: "Select", image: nil, enabled: true, perform: { actions += 1 })],
      dismissLabel: "Close", appearance: appearance, isValid: { true }, didDismiss: { _ in })
    XCTAssertTrue(menu.present(in: window))
    anchor.frame.size.width = 500
    menu.setNeedsLayout(); menu.layoutIfNeeded()
    menu.choose(0)
    XCTAssertNil(menu.superview); XCTAssertTrue(body.superview === anchor)
    XCTAssertEqual(body.frame, anchor.bounds); XCTAssertEqual(actions, 0)
  }
}
