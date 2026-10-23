import XCTest
import UIKit
@testable import ZoenCore

// These exercise production geometry/content and actual UIView removal.
// Fabric lookup, the live JS bridge, real Photos/camera permission and visual
// fidelity require the integrated app; they are outside this test harness.
@MainActor
final class AttachmentContainerTests: XCTestCase {
  let appearance = ZoenKeyboardPanelAppearance(background: .white, text: .black,
    secondary: .gray, accent: .blue, separator: .lightGray, bottomInset: 34)
  var model: [String: Any] {
    ["items": [["id": "ph://synthetic-a", "name": "a.jpg", "type": "image"]],
     "selected": ["ph://synthetic-a"], "labels": ["photos": "Gallery", "files": "File", "camera": "Camera", "close": "Close"],
     "albumId": NSNull(), "granted": true, "cameraAvailable": false,
     "caption": ["value": "", "placeholder": "Add a caption", "editable": true, "sending": false, "count": 1, "sendLabel": "Send"]]
  }
  func find<T: UIView>(_ view: UIView, type: T.Type, id: String) -> T? {
    if let value = view as? T, value.accessibilityIdentifier == id { return value }
    for child in view.subviews { if let match = find(child, type: type, id: id) { return match } }
    return nil
  }
  func content(_ events: @escaping (String, [String: Any]) -> Void) throws -> ZoenAttachmentKeyboardContent {
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
    let presenter = window.rootViewController!
    let background = UIView(frame: window.bounds)
    presenter.view.addSubview(background)
    window.isHidden = false
    defer { window.isHidden = true }
    XCTAssertFalse(background.accessibilityElementsHidden)
    let overlay = ZoenAttachmentContainer(content: OverlayContent(), appearance: appearance, requestClose: {})
    overlay.present(in: window)
    XCTAssertTrue(presenter.children.first?.view.superview === presenter.view)
    XCTAssertTrue(overlay.superview === presenter.children.first?.view)
    XCTAssertFalse(presenter.view.accessibilityElementsHidden)
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
      XCTAssertTrue(presenter.children.isEmpty)
      XCTAssertFalse(background.accessibilityElementsHidden, "restore only after actual overlay removal")
      completions += 1
    }
    XCTAssertEqual(completions, 1)
    overlay.dismiss(animated: false) { completions += 1 }
    XCTAssertEqual(completions, 1)
  }


  func testCaptionFocusExpandsWithoutSoftwareKeyboardAndCannotReviveAfterRetirement() throws {
    var events: [String] = []
    let content = try content { action, _ in events.append(action) }
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
    window.rootViewController = UIViewController()
    window.isHidden = false
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
    window.isHidden = false
    let overlay = ZoenAttachmentContainer(content: content, appearance: appearance, requestClose: {})
    overlay.present(in: window)
    defer { overlay.dismiss(animated: false, completion: {}); window.isHidden = true }
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

}
