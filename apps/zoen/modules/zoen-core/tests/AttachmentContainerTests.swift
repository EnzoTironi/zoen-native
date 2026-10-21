import XCTest
import UIKit
@testable import ZoenCore

// These exercise production geometry/content and actual UIView removal.
// Fabric lookup, the live JS bridge, real Photos/camera permission and visual
// fidelity require the integrated app; they are outside this test harness.
@MainActor
final class AttachmentContainerTests: XCTestCase {
  private let appearance = ZoenKeyboardPanelAppearance(background: .white, text: .black,
    secondary: .gray, accent: .blue, separator: .lightGray, bottomInset: 34)
  private var model: [String: Any] {
    ["items": [["id": "ph://synthetic-a", "name": "a.jpg", "type": "image"]],
     "selected": ["ph://synthetic-a"], "labels": ["photos": "Gallery", "files": "File", "camera": "Camera", "close": "Close"],
     "albumId": NSNull(), "granted": true, "cameraAvailable": false,
     "caption": ["value": "", "placeholder": "Add a caption", "editable": true, "sending": false, "count": 1, "sendLabel": "Send"]]
  }
  private func find<T: UIView>(_ view: UIView, type: T.Type, id: String) -> T? {
    if let value = view as? T, value.accessibilityIdentifier == id { return value }
    for child in view.subviews { if let match = find(child, type: type, id: id) { return match } }
    return nil
  }
  private func content(_ events: @escaping (String, [String: Any]) -> Void) throws -> ZoenAttachmentKeyboardContent {
    let content = ZoenAttachmentKeyboardContent(send: events)
    content.view.frame = CGRect(x: 0, y: 0, width: 402, height: 578)
    try content.update(model: model, appearance: appearance)
    content.updatePresentation(expansion: 0, keyboardHeight: 0, safeArea: UIEdgeInsets(top: 0, left: 0, bottom: 34, right: 0))
    content.view.layoutIfNeeded()
    return content
  }
  func testPinnedCollapsedInsetAdaptsToPortraitSmallPhoneAndLandscape() {
    XCTAssertEqual(ZoenAttachmentContainerMetrics.collapsedInset(size: CGSize(width: 402, height: 874), regular: false), 217)
    XCTAssertEqual(ZoenAttachmentContainerMetrics.collapsedInset(size: CGSize(width: 320, height: 568), regular: false), 85)
    XCTAssertEqual(ZoenAttachmentContainerMetrics.collapsedInset(size: CGSize(width: 874, height: 402), regular: false), 0)
    XCTAssertEqual(ZoenAttachmentContainerMetrics.collapsedInset(size: CGSize(width: 768, height: 1024), regular: true), 210)
    let collapsed = ZoenAttachmentContainerMetrics.layout(size: CGSize(width: 402, height: 874), baseTop: 69,
      offset: 217, inset: 217, landscape: false, regular: false, fullScreen: false, scale: 3)
    let expanded = ZoenAttachmentContainerMetrics.layout(size: CGSize(width: 402, height: 874), baseTop: 69,
      offset: 0, inset: 217, landscape: false, regular: false, fullScreen: false, scale: 3)
    XCTAssertEqual(collapsed.navigation.size.height, 805)
    XCTAssertEqual(expanded.navigation.size.height, 805, "sheet movement must never resize its controller viewport")
    XCTAssertEqual(collapsed.clip.height, 588)
    XCTAssertEqual(collapsed.clip.minY, 71 + 1.0 / 3, accuracy: 0.0001)
    XCTAssertEqual(collapsed.clipScale, 392.0 / 402, accuracy: 0.0001)
    XCTAssertEqual(collapsed.additionalBottomInset, 217)
    XCTAssertEqual(expanded.additionalBottomInset, 0)
    XCTAssertEqual(expanded.clipScale, 1)
  }
  func testReleaseKeepsScrollMomentumOutOfContainerAndUsesActualThresholds() {
    typealias Metrics = ZoenAttachmentContainerMetrics
    XCTAssertEqual(Metrics.destination(expanded: false, inset: 217, offset: 107, translation: -110, velocity: 0, innerOffset: 0), .expanded)
    XCTAssertEqual(Metrics.destination(expanded: false, inset: 217, offset: 217, translation: 0, velocity: -301, innerOffset: 0), .expanded)
    XCTAssertEqual(Metrics.destination(expanded: false, inset: 217, offset: 277, translation: 60, velocity: 0, innerOffset: 0), .collapsed)
    XCTAssertEqual(Metrics.destination(expanded: false, inset: 217, offset: 278, translation: 61, velocity: 0, innerOffset: 0), .dismissed)
    XCTAssertEqual(Metrics.destination(expanded: true, inset: 217, offset: 120, translation: 120, velocity: 0, innerOffset: 0), .collapsed)
    XCTAssertEqual(Metrics.destination(expanded: true, inset: 217, offset: 0, translation: 0, velocity: 1900, innerOffset: 0), .dismissed)
    XCTAssertEqual(Metrics.destination(expanded: true, inset: 217, offset: 0, translation: 0, velocity: 1900, innerOffset: 12), .expanded)
    XCTAssertEqual(Metrics.destination(expanded: true, inset: 0, offset: 0, translation: 0, velocity: 1900, innerOffset: 12), .expanded)
  }
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
  private final class OverlayContent: ZoenAttachmentOverlayContent {
    let view = UIView()
    let attachmentScrollView = UIScrollView()
    let attachmentHasKeyboardFocus = false
    let attachmentAnimatedViews: [UIView] = []
    var attachmentDidBeginEditing: (() -> Void)?
    var attachmentPresentationChanged: (() -> Void)?
    var attachmentPrefersFullScreen = false
    var attachmentHandlesVerticalGesture: Bool { attachmentPrefersFullScreen }
    func update(model: [String: Any], appearance: ZoenKeyboardPanelAppearance) throws {}
    func updatePresentation(expansion: CGFloat, keyboardHeight: CGFloat, safeArea: UIEdgeInsets) {}
    func retire() {}
    func retireForDismissal(completion: @escaping @MainActor () -> Void) { completion() }
  }
  func testNonAnimatedDismissalCompletesOnlyAfterActualRemovalAndExactlyOnce() {
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
    window.rootViewController = UIViewController()
    let background = window.rootViewController!.view!
    XCTAssertFalse(background.accessibilityElementsHidden)
    let overlay = ZoenAttachmentContainer(content: OverlayContent(), appearance: appearance, requestClose: {})
    overlay.present(in: window)
    XCTAssertTrue(overlay.superview === window)
    XCTAssertTrue(background.accessibilityElementsHidden)
    let sheet = find(overlay, type: UIView.self, id: "attachment-container-sheet")
    let wrapping = find(overlay, type: UIView.self, id: "attachment-container-wrapping")
    XCTAssertGreaterThan(sheet?.bounds.height ?? 0, 0)
    XCTAssertNil(sheet?.layer.animation(forKey: "zoen.attachment.motion.bounds"), "entrance must translate a full-height panel, never accordion its bounds")
    if !UIAccessibility.isReduceMotionEnabled {
      let position = wrapping?.layer.animation(forKey: "zoen.attachment.motion.position") as? CABasicAnimation
      XCTAssertEqual(position?.duration, 0.4)
      XCTAssertNotNil(position?.fromValue)
    }
    var completions = 0
    overlay.dismiss(animated: false) {
      XCTAssertNil(overlay.superview)
      XCTAssertFalse(background.accessibilityElementsHidden, "restore only after actual overlay removal")
      completions += 1
    }
    XCTAssertEqual(completions, 1)
    overlay.dismiss(animated: false) { completions += 1 }
    XCTAssertEqual(completions, 1)
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

  func testCaptionFocusExpandsWithoutSoftwareKeyboardAndCannotReviveAfterRetirement() throws {
    var events: [String] = []
    let content = try content { action, _ in events.append(action) }
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
    window.rootViewController = UIViewController()
    let overlay = ZoenAttachmentContainer(content: content, appearance: appearance, requestClose: {})
    overlay.present(in: window)
    defer { content.retire(); overlay.dismiss(animated: false, completion: {}); window.isHidden = true }
    let caption = try XCTUnwrap(find(content.view, type: UITextView.self, id: "attachment-caption"))
    let wrapping = try XCTUnwrap(find(overlay, type: UIView.self, id: "attachment-container-wrapping"))
    let collapsedTop = wrapping.frame.minY
    // Exercise the actual delegate callback with no keyboard frame notification.
    // Actual first-responder admission is separately checked in the app journey.
    content.textViewDidBeginEditing(caption)
    XCTAssertLessThan(wrapping.frame.minY, collapsedTop)
    if !UIAccessibility.isReduceMotionEnabled {
      let spring = try XCTUnwrap(wrapping.layer.animation(forKey: "zoen.attachment.motion.position") as? CASpringAnimation)
      XCTAssertEqual(spring.mass, 1)
      XCTAssertEqual(spring.stiffness, 555.027, accuracy: 0.001)
      XCTAssertEqual(spring.damping, 47.118, accuracy: 0.001)
      XCTAssertEqual(spring.duration, 0.5)
      XCTAssertFalse(spring.allowsOverdamping)
    }
    XCTAssertTrue(events.isEmpty, "focus cannot send or mutate caption contents")
    content.retire()
    XCTAssertNil(content.attachmentDidBeginEditing)
    content.textViewDidBeginEditing(caption)
    XCTAssertTrue(events.isEmpty)
  }

  func testBackgroundAccessibilityLeasePreservesOriginalValueAndOverlappingOwners() {
    let root = UIView()
    let first = ZoenAttachmentAccessibilityLease(root: root)
    let second = ZoenAttachmentAccessibilityLease(root: root)
    XCTAssertTrue(root.accessibilityElementsHidden)
    first.release()
    XCTAssertTrue(root.accessibilityElementsHidden, "another admitted owner still covers the root")
    second.release()
    XCTAssertFalse(root.accessibilityElementsHidden)
    second.release()
    XCTAssertFalse(root.accessibilityElementsHidden, "duplicate release cannot change another owner")
    root.accessibilityElementsHidden = true
    let alreadyHidden = ZoenAttachmentAccessibilityLease(root: root)
    alreadyHidden.release()
    XCTAssertTrue(root.accessibilityElementsHidden, "respect an originally hidden root")
  }

  func testFlatGalleryUsesWholeWindowAndRestoresPriorSheetWithoutAnotherModalEntrance() throws {
    let content = OverlayContent()
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
    window.rootViewController = UIViewController()
    let overlay = ZoenAttachmentContainer(content: content, appearance: appearance, requestClose: {})
    overlay.present(in: window)
    defer { overlay.dismiss(animated: false, completion: {}) }
    let sheet = try XCTUnwrap(find(overlay, type: UIView.self, id: "attachment-container-sheet"))
    let wrapping = try XCTUnwrap(find(overlay, type: UIView.self, id: "attachment-container-wrapping"))
    let navigation = try XCTUnwrap(find(overlay, type: UIView.self, id: "attachment-container-navigation"))
    let previousTop = wrapping.frame.minY
    let previousSize = sheet.bounds.size
    let previousViewport = navigation.bounds.size
    content.attachmentPrefersFullScreen = true
    content.attachmentPresentationChanged?()
    XCTAssertEqual(sheet.frame, window.bounds)
    XCTAssertEqual(sheet.layer.cornerRadius, 0)
    XCTAssertTrue(content.attachmentHandlesVerticalGesture)
    XCTAssertNil(sheet.layer.animation(forKey: "zoen.attachment.motion.position"), "flat children own the image transition, never another sheet entrance")
    content.attachmentPrefersFullScreen = false
    content.attachmentPresentationChanged?()
    XCTAssertEqual(wrapping.frame.minY, previousTop)
    XCTAssertEqual(sheet.bounds.size, previousSize)
    XCTAssertEqual(navigation.bounds.size, previousViewport)
    XCTAssertNil(sheet.layer.animation(forKey: "zoen.attachment.motion.position"))
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
