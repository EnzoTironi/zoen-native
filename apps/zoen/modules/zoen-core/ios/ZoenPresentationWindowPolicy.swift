import ExpoModulesCore
import UIKit

// Telegram-iOS f1dd7a2 ContextControllerImpl:1953–1973,1986–1995:
// extracted blur hides status; context locks the current orientation; dismissal
// restores inherited status immediately. UIKit/Expo own the system traits.
// No UIApplication status-bar setters, method swizzling or JS policy events.
enum ZoenPresentationPolicy { case inherited, referenceContext, extractedContext }

/// Marks native overlay children so they never displace the real native-stack
/// owner when the root forwards inherited window traits.
protocol ZoenPresentationOverlayController: AnyObject {}

public final class ZoenPresentationRootDelegate: ExpoReactDelegateHandler {
  public override func createRootViewController() -> UIViewController? {
    MainActor.assumeIsolated { ZoenPresentationRootController() }
  }
}

/// The app-delegate intersection also protects a current orientation when an
/// existing native-stack/system controller is presented above the React root.
/// Inherited masks preserve the generated phone/iPad Info.plist declaration.
public final class ZoenPresentationOrientationDelegate: ExpoAppDelegateSubscriber {
  public func application(_ application: UIApplication, supportedInterfaceOrientationsFor window: UIWindow?) -> UIInterfaceOrientationMask {
    MainActor.assumeIsolated {
      let allowed = ZoenPresentationRootController.allowedOrientations(info: Bundle.main.infoDictionary,
        idiom: window?.traitCollection.userInterfaceIdiom ?? UIDevice.current.userInterfaceIdiom)
      guard let root = window?.rootViewController as? ZoenPresentationRootController,
            let locked = root.lockedOrientation else { return allowed }
      return locked.intersection(allowed)
    }
  }
}

@MainActor
final class ZoenPresentationRootController: UIViewController {
  private final class WeakLease {
    weak var value: Lease?
    init(_ value: Lease) { self.value = value }
  }
  @MainActor final class Lease {
    private weak var root: ZoenPresentationRootController?
    private weak var window: UIWindow?
    private(set) weak var controller: UIViewController?
    let orientation: UIInterfaceOrientationMask?
    private(set) var hidesStatusBar: Bool
    private var suspended = false
    private var released = false
    fileprivate var active: Bool {
      !released && !suspended && window != nil && controller?.viewIfLoaded?.window === window &&
        window?.rootViewController === root
    }
    fileprivate init(root: ZoenPresentationRootController, controller: UIViewController, window: UIWindow, hidesStatusBar: Bool) {
      self.root = root; self.controller = controller; self.window = window
      self.hidesStatusBar = hidesStatusBar
      orientation = Self.mask(window.windowScene?.effectiveGeometry.interfaceOrientation)
    }
    static func mask(_ orientation: UIInterfaceOrientation?) -> UIInterfaceOrientationMask? {
      switch orientation {
      case .portrait: return .portrait
      case .portraitUpsideDown: return .portraitUpsideDown
      case .landscapeLeft: return .landscapeLeft
      case .landscapeRight: return .landscapeRight
      default: return nil
      }
    }
    func setSuspended(_ value: Bool) {
      guard !released, suspended != value else { return }
      suspended = value; root?.policyChanged()
    }
    func release() {
      guard !released else { return }
      released = true; root?.policyChanged()
    }
    func endStatusOwnership() {
      guard !released, hidesStatusBar else { return }
      hidesStatusBar = false; root?.policyChanged()
    }
  }
  private var leases: [WeakLease] = []
  private var topLease: Lease? { leases.reversed().compactMap(\.value).first { $0.active } }
  private var statusLease: Lease? { leases.reversed().compactMap(\.value).first { $0.active && $0.hidesStatusBar } }
  var lockedOrientation: UIInterfaceOrientationMask? { topLease?.orientation }
  func acquire(controller: UIViewController, window: UIWindow, hidesStatusBar: Bool) -> Lease {
    leases.removeAll { $0.value == nil }
    let lease = Lease(root: self, controller: controller, window: window, hidesStatusBar: hidesStatusBar)
    leases.append(WeakLease(lease)); policyChanged()
    return lease
  }
  // A dev-client wrapper can sit between the root and native-stack. Forward to
  // its real UIKit container instead of assuming a single child level. Native
  // navigation/tab containers retain their own active-screen trait resolution.
  private var navigationOwner: UIViewController? {
    var current: UIViewController = self
    while let child = current.children.last(where: { !($0 is ZoenPresentationOverlayController) && !$0.isBeingDismissed }) {
      if child is UINavigationController || child is UITabBarController { return child }
      if child.childForStatusBarStyle != nil || child.childForStatusBarHidden != nil { return child }
      current = child
    }
    return current === self ? nil : current
  }
  override var childForStatusBarStyle: UIViewController? { navigationOwner }
  override var childForStatusBarHidden: UIViewController? { statusLease?.controller ?? navigationOwner }
  override var childForHomeIndicatorAutoHidden: UIViewController? { navigationOwner }
  override var supportedInterfaceOrientations: UIInterfaceOrientationMask {
    lockedOrientation ?? navigationOwner?.supportedInterfaceOrientations ??
      Self.allowedOrientations(info: Bundle.main.infoDictionary, idiom: traitCollection.userInterfaceIdiom)
  }
  override var preferredStatusBarUpdateAnimation: UIStatusBarAnimation {
    statusLease != nil ? .none : (navigationOwner?.preferredStatusBarUpdateAnimation ?? .none)
  }
  fileprivate func policyChanged() {
    // The pin assigns statusBarStyle directly: StatusBar.swift:52–60 emits
    // .immediate. Its separate animated setter is not the Context path.
    setNeedsStatusBarAppearanceUpdate()
    setNeedsUpdateOfSupportedInterfaceOrientations()
    var presented = presentedViewController
    while let controller = presented {
      controller.setNeedsUpdateOfSupportedInterfaceOrientations()
      presented = controller.presentedViewController
    }
  }
  static func allowedOrientations(info: [String: Any]?, idiom: UIUserInterfaceIdiom) -> UIInterfaceOrientationMask {
    func parse(_ key: String) -> UIInterfaceOrientationMask? {
      guard let values = info?[key] as? [String] else { return nil }
      let mask = values.reduce(into: UIInterfaceOrientationMask()) { result, value in
        switch value {
        case "UIInterfaceOrientationPortrait": result.insert(.portrait)
        case "UIInterfaceOrientationPortraitUpsideDown": result.insert(.portraitUpsideDown)
        case "UIInterfaceOrientationLandscapeLeft": result.insert(.landscapeLeft)
        case "UIInterfaceOrientationLandscapeRight": result.insert(.landscapeRight)
        default: break
        }
      }
      return mask.isEmpty ? nil : mask
    }
    if idiom == .pad, let mask = parse("UISupportedInterfaceOrientations~ipad") { return mask }
    return parse("UISupportedInterfaceOrientations") ?? (idiom == .pad ? .all : .allButUpsideDown)
  }
}
