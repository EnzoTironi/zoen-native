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

extension RecentAttachmentContentTests {
  func testEmptyIntroSeparatesIntrinsicPanelFromCollapsedInsetAndNeverOverlapsActions() throws {
    let cases: [(CGSize, CGFloat, CGFloat)] = [(CGSize(width: 402, height: 810), 308, 218), (CGSize(width: 375, height: 620), 254, 154), (CGSize(width: 844, height: 390), 90, 0)]
    for (size, bottom, additional) in cases {
      for category in ["document", "audio"] {
        let content = ZoenAttachmentFileContent { _, _ in }
        let window = UIWindow(frame: CGRect(origin: .zero, size: size))
        let controller = UIViewController(); window.rootViewController = controller
        controller.view.addSubview(content.view); window.isHidden = false
        defer { window.isHidden = true; content.retire() }
        content.view.frame = CGRect(origin: .zero, size: size)
        try content.update(model: ["revision": "geometry", "activeCategory": category, "fileItems": [[String: Any]](), "audioItems": [[String: Any]](), "recent": ["selected": [String](), "query": "", "searching": false, "loading": false, "hasMore": false, "error": false], "labels": ["filesIntro": "Choose a file from your device or iCloud Drive.", "chooseGallery": "Gallery", "browseFiles": "Browse"]], appearance: ZoenKeyboardPanelAppearance(background: .black, text: .white, secondary: .gray, accent: .systemBlue, separator: .darkGray, bottomInset: 0))
        content.setVisible(true)
        content.updateLayout(navigationHeight: 68, bottomInset: bottom, safeArea: .zero, additionalBottomInset: additional)
        content.scrollView.layoutIfNeeded()
        let last = try XCTUnwrap(content.scrollView.cellForRow(at: IndexPath(row: category == "document" ? 1 : 0, section: 0)))
        let actionsBottom = last.convert(last.bounds, to: content.view).maxY
        let art = try XCTUnwrap(content.view.subviews.first { $0.accessibilityIdentifier == "attachment-file-empty-art" })
        let text = try XCTUnwrap(content.view.subviews.first { $0.accessibilityIdentifier == "attachment-file-empty-text" })
        if !art.isHidden { XCTAssertGreaterThanOrEqual(art.frame.minY, actionsBottom + 12) }
        if !text.isHidden { XCTAssertGreaterThanOrEqual(text.frame.minY, actionsBottom + 12); XCTAssertLessThanOrEqual(text.frame.maxY, size.height - bottom) }
        if size.width >= size.height { XCTAssertTrue(art.isHidden) }
        content.retire()
      }
    }
  }
}
