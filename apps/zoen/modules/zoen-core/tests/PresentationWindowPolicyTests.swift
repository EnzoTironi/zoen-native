import XCTest
import UIKit
@testable import ZoenCore

@MainActor final class PresentationWindowPolicyTests: XCTestCase {
  private final class Route: UIViewController {
    override var preferredStatusBarStyle: UIStatusBarStyle { .lightContent }
    override var supportedInterfaceOrientations: UIInterfaceOrientationMask { .landscape }
    override var preferredStatusBarUpdateAnimation: UIStatusBarAnimation { .none }
  }
  private func mount() -> (UIWindow, ZoenPresentationRootController, UINavigationController) {
    let root = ZoenPresentationRootDelegate().createRootViewController() as! ZoenPresentationRootController
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874)); window.rootViewController = root
    let navigation = UINavigationController(rootViewController: Route())
    root.addChild(navigation); root.view.addSubview(navigation.view); navigation.view.frame = root.view.bounds
    navigation.didMove(toParent: root); window.isHidden = false
    return (window, root, navigation)
  }
  func testRealFactoryRootForwardsNativeContainerAndReleasesPolicyAtDismissalStart() throws {
    let (window, root, navigation) = mount(); defer { window.isHidden = true }
    XCTAssertTrue(root.childForStatusBarStyle === navigation)
    XCTAssertTrue(root.childForStatusBarHidden === navigation)
    let content = UIView()
    let owner = try XCTUnwrap(ZoenAttachmentPresentationOwner(content: content, in: window,
      policy: .extractedContext, onWindowLoss: {}))
    XCTAssertTrue(owner.isAttached(in: window)); XCTAssertTrue(root.childForStatusBarHidden?.prefersStatusBarHidden == true)
    XCTAssertTrue(root.childForStatusBarStyle === navigation)
    XCTAssertEqual(root.lockedOrientation, ZoenPresentationRootController.Lease.mask(window.windowScene?.effectiveGeometry.interfaceOrientation))
    let locked = root.lockedOrientation
    owner.beginWindowPolicyDismissal()
    XCTAssertTrue(owner.isAttached(in: window)) // The source's .2s return still owns its actual view.
    XCTAssertTrue(root.childForStatusBarHidden === navigation); XCTAssertEqual(root.lockedOrientation, locked)
    owner.remove(); owner.remove()
    XCTAssertTrue(root.children.elementsEqual([navigation])); XCTAssertNil(content.superview)
  }
  func testOverlappingOwnersSuspensionAndWindowLossCannotLeaveStatusOwner() throws {
    let (window, root, navigation) = mount(); defer { window.isHidden = true }
    let first = try XCTUnwrap(ZoenAttachmentPresentationOwner(content: UIView(), in: window,
      policy: .extractedContext, onWindowLoss: {}))
    let second = try XCTUnwrap(ZoenAttachmentPresentationOwner(content: UIView(), in: window,
      policy: .extractedContext, onWindowLoss: {}))
    let top = root.childForStatusBarHidden
    first.beginWindowPolicyDismissal(); XCTAssertTrue(root.childForStatusBarHidden === top)
    second.setSuspended(true); XCTAssertTrue(root.childForStatusBarHidden === navigation)
    second.setSuspended(false); XCTAssertTrue(root.childForStatusBarHidden === top)
    second.remove(); first.remove()
    XCTAssertTrue(root.childForStatusBarHidden === navigation); XCTAssertNil(root.lockedOrientation)
    var losses = 0
    let lost = try XCTUnwrap(ZoenAttachmentPresentationOwner(content: UIView(), in: window,
      policy: .extractedContext, onWindowLoss: { losses += 1 }))
    root.view.removeFromSuperview()
    XCTAssertEqual(losses, 1); XCTAssertFalse(lost.isAttached(in: window))
    XCTAssertTrue(root.childForStatusBarHidden === navigation); XCTAssertNil(root.lockedOrientation)
  }
  func testOrdinaryOverlayDoesNotAcquirePolicyAndInvalidRootCannotPretendToHideStatus() throws {
    let (window, root, navigation) = mount(); defer { window.isHidden = true }
    let owner = try XCTUnwrap(ZoenAttachmentPresentationOwner(content: UIView(), in: window, onWindowLoss: {}))
    XCTAssertTrue(root.childForStatusBarHidden === navigation); XCTAssertNil(root.lockedOrientation)
    owner.remove()
    let otherWindow = UIWindow(frame: window.bounds); otherWindow.rootViewController = UIViewController(); otherWindow.isHidden = false
    defer { otherWindow.isHidden = true }
    let content = UIView()
    XCTAssertNil(ZoenAttachmentPresentationOwner(content: content, in: otherWindow,
      policy: .extractedContext, onWindowLoss: {}))
    XCTAssertNil(content.superview)
  }
  func testReferenceLocksOrientationButInheritsUnderlyingStatusThroughOverlapAndDismissal() throws {
    let (window, root, navigation) = mount(); defer { window.isHidden = true }
    let extracted = try XCTUnwrap(ZoenAttachmentPresentationOwner(content: UIView(), in: window,
      policy: .extractedContext, onWindowLoss: {}))
    let hidden = root.childForStatusBarHidden
    let reference = try XCTUnwrap(ZoenAttachmentPresentationOwner(content: UIView(), in: window,
      policy: .referenceContext, onWindowLoss: {}))
    let locked = root.lockedOrientation
    XCTAssertTrue(root.childForStatusBarHidden === hidden)
    extracted.beginWindowPolicyDismissal()
    XCTAssertTrue(root.childForStatusBarHidden === navigation); XCTAssertEqual(root.lockedOrientation, locked)
    reference.beginWindowPolicyDismissal()
    XCTAssertEqual(root.lockedOrientation, locked)
    extracted.remove(); reference.remove()
    XCTAssertTrue(root.childForStatusBarHidden === navigation); XCTAssertNil(root.lockedOrientation)
  }
  func testPhonePadMasksUnknownEntriesAndExactCurrentInterfaceOrientationMapping() {
    let info: [String: Any] = ["UISupportedInterfaceOrientations": ["UIInterfaceOrientationPortrait", "UIInterfaceOrientationLandscapeLeft", "UIInterfaceOrientationLandscapeRight"],
      "UISupportedInterfaceOrientations~ipad": ["UIInterfaceOrientationPortraitUpsideDown", "UIInterfaceOrientationLandscapeRight"]]
    XCTAssertEqual(ZoenPresentationRootController.allowedOrientations(info: info, idiom: .phone), [.portrait, .landscape])
    XCTAssertEqual(ZoenPresentationRootController.allowedOrientations(info: info, idiom: .pad), [.portraitUpsideDown, .landscapeRight])
    var invalid = info; invalid["UISupportedInterfaceOrientations~ipad"] = ["invalid"]
    XCTAssertEqual(ZoenPresentationRootController.allowedOrientations(info: invalid, idiom: .pad), [.portrait, .landscape])
    for (orientation, mask) in [(UIInterfaceOrientation.portrait, UIInterfaceOrientationMask.portrait), (.portraitUpsideDown, .portraitUpsideDown), (.landscapeLeft, .landscapeLeft), (.landscapeRight, .landscapeRight)] {
      XCTAssertEqual(ZoenPresentationRootController.Lease.mask(orientation), mask)
    }
    XCTAssertNil(ZoenPresentationRootController.Lease.mask(.unknown)); XCTAssertNil(ZoenPresentationRootController.Lease.mask(nil))
  }
}
