import XCTest
import UIKit

/// Actual production table/cell/UIControl callbacks. No private cell replacement
/// or source-text assertion; an isolated UIKit runner owns this test window.
@MainActor final class RecentAttachmentContentTests: XCTestCase {
  private final class Events { var values: [(String, [String: Any])] = [] }
  private func model(_ revision: String, busy: Bool = false, loading: Bool = false) -> [String: Any] {
    ["revision": revision, "activeCategory": "audio", "busy": busy,
     "audioItems": [["id": "source-message", "name": "track.m4a", "mime": "audio/mp4", "bytes": "12", "at_ms": "10", "author_title": "Author"]],
     "recent": ["selected": [String](), "query": "", "searching": false, "loading": loading, "hasMore": false, "error": false],
     "labels": ["previewFile": "Preview", "browseFiles": "Browse", "chatsMusic": "Music"]]
  }
  private var appearance: ZoenKeyboardPanelAppearance {
    ZoenKeyboardPanelAppearance(background: .black, text: .white, secondary: .gray, accent: .systemBlue, separator: .darkGray, bottomInset: 0)
  }
  private func mounted(_ initial: [String: Any]) throws -> (UIWindow, ZoenAttachmentFileContent, Events, UITableViewCell, UIButton) {
    let events = Events(), content = ZoenAttachmentFileContent { events.values.append(($0, $1)) }
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
    let controller = UIViewController(); window.rootViewController = controller
    controller.view.addSubview(content.view); content.view.frame = window.bounds
    try content.update(model: initial, appearance: appearance); content.setVisible(true)
    content.updateLayout(navigationHeight: 56, bottomInset: 90, safeArea: .zero)
    window.isHidden = false; controller.view.layoutIfNeeded(); content.scrollView.layoutIfNeeded()
    let cell = try XCTUnwrap(content.scrollView.cellForRow(at: IndexPath(row: 0, section: 1)))
    let play = try XCTUnwrap(cell.contentView.subviews.compactMap { $0 as? UIButton }.first { $0.accessibilityLabel == "Preview" })
    return (window, content, events, cell, play)
  }
  func testRevisionOnlyUpdateRebindsExistingVisiblePlayWithoutRemount() throws {
    let (window, content, events, cell, play) = try mounted(model("r1")); defer { window.isHidden = true; content.retire() }
    play.sendActions(for: .touchUpInside)
    XCTAssertEqual(events.values.last?.1["revision"] as? String, "r1")
    try content.update(model: model("r2", loading: true), appearance: appearance)
    XCTAssertTrue(content.scrollView.cellForRow(at: IndexPath(row: 0, section: 1)) === cell)
    play.sendActions(for: .touchUpInside)
    XCTAssertEqual(events.values.count, 2)
    XCTAssertEqual(events.values.last?.0, "recentPreview")
    XCTAssertEqual(events.values.last?.1["id"] as? String, "source-message")
    XCTAssertEqual(events.values.last?.1["revision"] as? String, "r2")
  }
  func testBusyToIdleRefreshesVisibleInteractionAndCurrentRevision() throws {
    let (window, content, events, cell, play) = try mounted(model("busy", busy: true)); defer { window.isHidden = true; content.retire() }
    XCTAssertFalse(cell.isUserInteractionEnabled)
    play.sendActions(for: .touchUpInside); XCTAssertTrue(events.values.isEmpty)
    try content.update(model: model("idle"), appearance: appearance)
    XCTAssertTrue(content.scrollView.cellForRow(at: IndexPath(row: 0, section: 1)) === cell)
    XCTAssertTrue(cell.isUserInteractionEnabled); XCTAssertEqual(cell.contentView.alpha, 1)
    play.sendActions(for: .touchUpInside)
    XCTAssertEqual(events.values.count, 1); XCTAssertEqual(events.values.last?.1["revision"] as? String, "idle")
  }
  func testHiddenAndRetiredVisibleControlsCannotEmit() throws {
    let (window, content, events, _, play) = try mounted(model("r1")); defer { window.isHidden = true }
    content.setVisible(false); play.sendActions(for: .touchUpInside); XCTAssertTrue(events.values.isEmpty)
    content.setVisible(true); content.retire(); play.sendActions(for: .touchUpInside); XCTAssertTrue(events.values.isEmpty)
  }
}
