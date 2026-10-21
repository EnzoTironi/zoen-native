import XCTest
import UIKit
@testable import ZoenCore

/// Actual UIKit parent/view/responder ownership; menu gestures are separately
/// executed by the bounded application/XCUI fixture against this same source.
@MainActor final class AttachmentPresentationTests: XCTestCase {
  private final class Content: ZoenAttachmentOverlayContent {
    let view = UIView()
    let attachmentScrollView = UIScrollView()
    let attachmentHasKeyboardFocus = false
    let attachmentAnimatedViews: [UIView] = []
    var attachmentDidBeginEditing: (() -> Void)?
    var attachmentPresentationChanged: (() -> Void)?
    func update(model: [String: Any], appearance: ZoenKeyboardPanelAppearance) throws {}
    func updatePresentation(expansion: CGFloat, keyboardHeight: CGFloat, safeArea: UIEdgeInsets) {}
    func retire() {}
    func retireForDismissal(completion: @escaping @MainActor () -> Void) { completion() }
  }
  private let appearance = ZoenKeyboardPanelAppearance(background: .black, text: .white, secondary: .gray,
    accent: .blue, separator: .darkGray, bottomInset: 34)
  private func window() -> (UIWindow, UIViewController, UIView, UIView) {
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
    let parent = UIViewController(); window.rootViewController = parent
    let content = UIView(frame: window.bounds), prehidden = UIView()
    prehidden.accessibilityElementsHidden = true
    parent.view.addSubview(content); parent.view.addSubview(prehidden)
    window.isHidden = false
    return (window, parent, content, prehidden)
  }
  func testActualControllerOwnsResponderAndRestoresOnlyPriorSiblingsAfterLastOwner() throws {
    let (window, parent, underlying, prehidden) = window(); defer { window.isHidden = true }
    let first = ZoenAttachmentContainer(content: Content(), appearance: appearance, requestClose: {})
    let second = ZoenAttachmentContainer(content: Content(), appearance: appearance, requestClose: {})
    first.present(in: window); second.present(in: window)
    XCTAssertEqual(parent.children.count, 2)
    XCTAssertTrue(first.superview?.next === parent.children[0]); XCTAssertTrue(second.superview?.next === parent.children[1])
    XCTAssertTrue(first.superview === parent.children[0].view); XCTAssertFalse(parent.view.accessibilityElementsHidden)
    XCTAssertTrue(underlying.accessibilityElementsHidden); XCTAssertTrue(prehidden.accessibilityElementsHidden)
    XCTAssertFalse(first.accessibilityElementsHidden); XCTAssertFalse(second.accessibilityElementsHidden)
    let laterSystemSurface = UIView(); parent.view.addSubview(laterSystemSurface)
    XCTAssertFalse(laterSystemSurface.accessibilityElementsHidden)
    first.dismiss(animated: false, completion: {})
    XCTAssertTrue(underlying.accessibilityElementsHidden); XCTAssertEqual(parent.children.count, 1)
    second.dismiss(animated: false, completion: {})
    XCTAssertFalse(underlying.accessibilityElementsHidden); XCTAssertTrue(prehidden.accessibilityElementsHidden)
    XCTAssertTrue(parent.children.isEmpty); XCTAssertFalse(laterSystemSurface.accessibilityElementsHidden)
  }
  func testParentWindowLossDetachesControllerAndDoesNotRetainItOrReviveLeases() throws {
    let (window, parent, underlying, _) = window(); defer { window.isHidden = true }
    let overlay = ZoenAttachmentContainer(content: Content(), appearance: appearance, requestClose: {})
    weak var child: UIViewController?
    autoreleasepool {
      overlay.present(in: window)
      child = parent.children.first
      XCTAssertNotNil(child)
      parent.view.removeFromSuperview()
      XCTAssertNil(overlay.window); XCTAssertNil(overlay.superview)
      XCTAssertTrue(parent.children.isEmpty); XCTAssertNil(child?.parent)
    }
    XCTAssertNil(child, "UIKit temporary ownership must drain without a controller/view cycle")
    XCTAssertFalse(underlying.accessibilityElementsHidden)
    var completions = 0
    overlay.dismiss(animated: true) { completions += 1 }
    XCTAssertEqual(completions, 1)
    XCTAssertFalse(underlying.accessibilityElementsHidden)
  }
  func testReopenCreatesFreshControllerAndPreservesWholeWindowGeometry() throws {
    let (window, parent, underlying, _) = window(); defer { window.isHidden = true }
    for _ in 0..<2 {
      let overlay = ZoenAttachmentContainer(content: Content(), appearance: appearance, requestClose: {})
      weak var child: UIViewController?
      autoreleasepool {
        overlay.present(in: window)
        XCTAssertEqual(overlay.convert(overlay.bounds, to: window), window.bounds)
        child = parent.children.first
        overlay.dismiss(animated: false, completion: {})
        XCTAssertNil(child?.parent); XCTAssertTrue(parent.children.isEmpty)
      }
      XCTAssertNil(child, "drain UIKit autoreleased child-list snapshots before checking lifetime")
      XCTAssertFalse(underlying.accessibilityElementsHidden)
    }
  }
  func testSharedCoverSuspensionAndChildPresentationKeepOriginalAccessibilityAndAttachmentAuthority() throws {
    let (window, parent, underlying, prehidden) = window(); defer { window.isHidden = true }
    let cover = UIView(); var lost = 0
    let owner = try XCTUnwrap(ZoenAttachmentPresentationOwner(content: cover, in: window, onWindowLoss: { lost += 1 }))
    XCTAssertTrue(owner.isAttached(in: window)); XCTAssertTrue(owner.updateFrame(in: window))
    XCTAssertTrue(underlying.accessibilityElementsHidden)
    owner.setSuspended(true)
    XCTAssertTrue(cover.isHidden); XCTAssertFalse(underlying.accessibilityElementsHidden)
    XCTAssertTrue(prehidden.accessibilityElementsHidden); XCTAssertTrue(owner.isAttached(in: window))
    let childCover = UIView()
    let child = try XCTUnwrap(ZoenAttachmentPresentationOwner(content: childCover, in: window, onWindowLoss: {}))
    XCTAssertFalse(childCover.accessibilityElementsHidden)
    owner.setSuspended(false)
    XCTAssertFalse(cover.isHidden); XCTAssertTrue(underlying.accessibilityElementsHidden)
    child.remove()
    XCTAssertTrue(underlying.accessibilityElementsHidden, "the resumed owner still holds its prior-sibling lease")
    let laterMenu = UIView(); parent.view.addSubview(laterMenu)
    XCTAssertFalse(laterMenu.accessibilityElementsHidden)
    parent.view.removeFromSuperview()
    XCTAssertEqual(lost, 1); XCTAssertFalse(owner.isAttached(in: window)); XCTAssertFalse(owner.updateFrame(in: window))
    XCTAssertNil(cover.superview); XCTAssertTrue(parent.children.isEmpty)
    XCTAssertFalse(underlying.accessibilityElementsHidden); XCTAssertTrue(prehidden.accessibilityElementsHidden)
    XCTAssertFalse(laterMenu.accessibilityElementsHidden)
    owner.setSuspended(false); owner.remove()
    XCTAssertEqual(lost, 1); XCTAssertFalse(underlying.accessibilityElementsHidden)
  }

}
