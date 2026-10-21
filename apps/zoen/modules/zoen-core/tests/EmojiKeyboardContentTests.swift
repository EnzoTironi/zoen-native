import XCTest
import UIKit
@testable import ZoenCore

// AUTHORED, NOT_RUN. These qualify only the content/event boundary once executed;
// exact Fabric UITextView/inputView ownership and keyboard geometry belong to
// the shared host's native tests and an actual integrated iPhone journey.
@MainActor
final class EmojiKeyboardContentTests: XCTestCase {
  private let appearance = ZoenKeyboardPanelAppearance(background: .white, text: .black,
    secondary: .gray, accent: .blue, separator: .lightGray, bottomInset: 34)
  private var model: [String: Any] {
    var labels = Dictionary(uniqueKeysWithValues: ZoenEmojiKeyboardData.sections.map { ($0.key, $0.key) })
    for key in ["keyboard", "delete", "recent", "variants"] { labels[key] = key }
    return ["emojiLabels": labels, "reducedMotion": true]
  }
  private func find<T: UIView>(_ view: UIView, type: T.Type, id: String? = nil) -> T? {
    if let value = view as? T, id == nil || value.accessibilityIdentifier == id { return value }
    for child in view.subviews { if let match = find(child, type: type, id: id) { return match } }
    return nil
  }
  func testPinnedUnicodeCategoriesAndToneVariantsRemainExact() {
    XCTAssertEqual(ZoenEmojiKeyboardData.sections.map(\.key), ["people", "animalsAndNature", "foodAndDrink", "activityAndSport", "travelAndPlaces", "objects", "symbols", "flags"])
    XCTAssertEqual(ZoenEmojiKeyboardData.sections.map { $0.emoji.count }, [411, 214, 130, 84, 131, 119, 302, 260])
    XCTAssertEqual(ZoenEmojiKeyboardData.allowed.count, 3491)
    XCTAssertEqual(ZoenEmojiKeyboardData.variants["👍"]?.count, 5)
    XCTAssertTrue(ZoenEmojiKeyboardData.allowed.contains("👍🏽"))
    XCTAssertTrue(ZoenEmojiKeyboardData.allowed.contains("👨‍👩‍👧‍👦"))
  }
  func testEmojiFactoryAllowsPinnedJoinersAndRefusesArbitraryText() {
    let host = ZoenEmojiKeyboardPanelHost()
    XCTAssertTrue(host.allowsTextEditingActions)
    XCTAssertTrue(host.admitsInsertion("👨‍👩‍👧‍👦"))
    XCTAssertTrue(host.admitsInsertion("👍🏽"))
    XCTAssertFalse(host.admitsInsertion("untrusted text"))
    XCTAssertFalse(host.admitsInsertion("\u{202E}text"))
    XCTAssertFalse(host.admitsInsertion("\n"))
  }
  func testSelectionEmitsOnlyExactNativeEmojiAndRetirementRefusesRetainedActivation() throws {
    var events: [(String, [String: Any])] = []
    let content = ZoenEmojiKeyboardContent { events.append(($0, $1)) }
    content.view.frame = CGRect(x: 0, y: 0, width: 402, height: 300)
    try content.update(model: model, appearance: appearance)
    content.view.layoutIfNeeded()
    let grid = try XCTUnwrap(find(content.view, type: UICollectionView.self))
    XCTAssertTrue(events.isEmpty, "loading/theme/layout never inserts text")
    let first = IndexPath(item: 0, section: 1)
    let cell = content.collectionView(grid, cellForItemAt: first)
    content.collectionView(grid, didSelectItemAt: first)
    XCTAssertEqual(events.count, 1)
    XCTAssertEqual(events.first?.0, "insert")
    XCTAssertEqual(events.first?.1["text"] as? String, "😀")
    XCTAssertEqual(Set(events.first?.1.keys.map { $0 } ?? []), ["text"])
    content.retire()
    content.collectionView(grid, didSelectItemAt: first)
    _ = cell.accessibilityActivate()
    XCTAssertEqual(events.count, 1)
    XCTAssertFalse(content.view.isUserInteractionEnabled)
  }
  func testKeyboardAndDeleteAreBoundedEventsAndNeverSupplyReplacementText() throws {
    var events: [(String, [String: Any])] = []
    let content = ZoenEmojiKeyboardContent { events.append(($0, $1)) }
    try content.update(model: model, appearance: appearance)
    let keyboard = try XCTUnwrap(find(content.view, type: UIButton.self, id: "composer-emoji-system-keyboard"))
    let delete = try XCTUnwrap(find(content.view, type: UIButton.self, id: "composer-emoji-delete"))
    delete.sendActions(for: .touchUpInside)
    keyboard.sendActions(for: .touchUpInside)
    XCTAssertEqual(events.map(\.0), ["delete", "keyboard"])
    XCTAssertTrue(events.allSatisfy { $0.1.isEmpty })
    content.retire()
    delete.sendActions(for: .touchUpInside)
    keyboard.sendActions(for: .touchUpInside)
    XCTAssertEqual(events.count, 2)
  }
  func testMissingOrUnboundedPresentationLabelsCannotActivatePanel() {
    let content = ZoenEmojiKeyboardContent { _, _ in XCTFail("invalid model emitted an edit") }
    XCTAssertThrowsError(try content.update(model: [:], appearance: appearance))
    XCTAssertFalse(content.view.isUserInteractionEnabled)
    var invalid = model
    var labels = invalid["emojiLabels"] as! [String: String]
    labels["delete"] = String(repeating: "x", count: 129)
    invalid["emojiLabels"] = labels
    XCTAssertThrowsError(try content.update(model: invalid, appearance: appearance))
    content.retire()
    XCTAssertThrowsError(try content.update(model: model, appearance: appearance))
  }
  func testCategoryNavigationChangesOnlyContentAndCannotInsertOrBecomeResponder() throws {
    var events = 0
    let content = ZoenEmojiKeyboardContent { _, _ in events += 1 }
    content.view.frame = CGRect(x: 0, y: 0, width: 402, height: 300)
    try content.update(model: model, appearance: appearance)
    content.view.layoutIfNeeded()
    let flags = try XCTUnwrap(find(content.view, type: UIButton.self, id: "composer-emoji-category-flags"))
    flags.sendActions(for: .touchUpInside)
    XCTAssertEqual(events, 0)
    XCTAssertFalse(content.view.isFirstResponder)
    let grid = try XCTUnwrap(find(content.view, type: UICollectionView.self))
    XCTAssertEqual(content.collectionView(grid, numberOfItemsInSection: 8), 260)
    XCTAssertFalse(grid.isFirstResponder)
    content.retire()
  }
}
