import UIKit

@MainActor
final class ZoenAttachmentAccessibilityLease {
  private final class State {
    weak var root: UIView?
    let original: Bool
    var owners = Set<UUID>()
    init(root: UIView) { self.root = root; original = root.accessibilityElementsHidden }
  }
  private static var states: [ObjectIdentifier: State] = [:]
  private let key: ObjectIdentifier
  private let token = UUID()
  private weak var root: UIView?
  private var released = false

  init(root: UIView) {
    Self.states = Self.states.filter { $0.value.root != nil }
    self.root = root
    key = ObjectIdentifier(root)
    let state: State
    if let existing = Self.states[key], existing.root === root { state = existing }
    else { state = State(root: root); Self.states[key] = state }
    state.owners.insert(token)
    root.accessibilityElementsHidden = true
  }
  func release() {
    guard !released else { return }
    released = true
    guard let root, let state = Self.states[key], state.root === root else { return }
    state.owners.remove(token)
    if state.owners.isEmpty {
      root.accessibilityElementsHidden = state.original
      Self.states.removeValue(forKey: key)
    }
  }
}

/// Reusable owner for native attachment surfaces that need UIKit menu presenters.
/// The existing controller owns a clear child wrapper; content and owner keep no
/// strong backreferences to that controller. It never moves React-owned views.
@MainActor
final class ZoenAttachmentPresentationOwner {
  private final class Background {
    weak var view: UIView?
    init(_ view: UIView) { self.view = view }
  }
  private weak var content: UIView?
  private weak var controller: AttachmentPresentationController?
  private weak var parent: UIViewController?
  private weak var admittedWindow: UIWindow?
  private var backgrounds: [Background] = []
  private var accessibilityLeases: [ZoenAttachmentAccessibilityLease] = []
  private var suspended = false
  private var removed = false
  private var onWindowLoss: (() -> Void)?
  private var windowPolicy: ZoenPresentationRootController.Lease?

  init?(content: UIView, in window: UIWindow, policy: ZoenPresentationPolicy = .inherited,
        onWindowLoss: @escaping () -> Void) {
    guard let parent = Self.presenter(in: window), content.superview == nil,
          policy == .inherited || (window.rootViewController is ZoenPresentationRootController && parent === window.rootViewController) else { return nil }
    self.content = content; self.parent = parent; admittedWindow = window
    self.onWindowLoss = onWindowLoss
    // Capture once. Resuming cannot accidentally hide a later system menu or
    // admitted child picker, and overlapping owners reuse the same AX leases.
    backgrounds = parent.view.subviews.filter {
      !($0 is AttachmentPresentationView) && !($0 is ZoenAttachmentContainer)
    }.map(Background.init)
    let controller = AttachmentPresentationController(content: content)
    self.controller = controller
    controller.host.onWindowLoss = { [weak self] in self?.windowLost() }
    parent.addChild(controller)
    controller.beginAppearanceTransition(true, animated: false)
    controller.host.frame = parent.view.convert(window.bounds, from: window)
    content.frame = controller.host.bounds
    parent.view.addSubview(controller.host)
    controller.didMove(toParent: parent)
    controller.endAppearanceTransition()
    if policy != .inherited, let root = window.rootViewController as? ZoenPresentationRootController {
      controller.hidesStatusBar = policy == .extractedContext
      windowPolicy = root.acquire(controller: controller, window: window, hidesStatusBar: controller.hidesStatusBar)
    }
    acquireAccessibility()
  }
  func isAttached(in window: UIWindow) -> Bool {
    guard !removed, admittedWindow === window, let content, let controller, let parent else { return false }
    return controller.parent === parent && controller.host.superview === parent.view &&
      content.superview === controller.host && content.window === window
  }
  @discardableResult func updateFrame(in window: UIWindow) -> Bool {
    guard isAttached(in: window), let controller, let parent, let content else { return false }
    controller.host.frame = parent.view.convert(window.bounds, from: window)
    content.frame = controller.host.bounds
    return true
  }
  func setSuspended(_ value: Bool) {
    guard !removed, suspended != value else { return }
    suspended = value
    content?.isHidden = value
    controller?.host.isHidden = value
    windowPolicy?.setSuspended(value)
    if value { releaseAccessibility() } else { acquireAccessibility() }
  }
  func remove() {
    guard !removed else { return }
    removed = true; onWindowLoss = nil
    controller?.hidesStatusBar = false
    windowPolicy?.release(); windowPolicy = nil
    if let controller {
      self.controller = nil
      controller.host.onWindowLoss = nil
      controller.willMove(toParent: nil)
      controller.beginAppearanceTransition(false, animated: false)
      content?.removeFromSuperview()
      controller.host.removeFromSuperview()
      controller.removeFromParent()
      controller.endAppearanceTransition()
    } else { content?.removeFromSuperview() }
    releaseAccessibility()
    backgrounds.removeAll()
  }
  func beginWindowPolicyDismissal() {
    controller?.hidesStatusBar = false
    windowPolicy?.endStatusOwnership()
  }
  private func windowLost() {
    guard !removed else { return }
    let notify = onWindowLoss
    remove()
    notify?()
  }
  private func acquireAccessibility() {
    guard !removed, !suspended, accessibilityLeases.isEmpty,
          let window = admittedWindow, isAttached(in: window), let parent else { return }
    accessibilityLeases = backgrounds.compactMap { background in
      guard let view = background.view, view.superview === parent.view else { return nil }
      return ZoenAttachmentAccessibilityLease(root: view)
    }
  }
  private func releaseAccessibility() {
    accessibilityLeases.forEach { $0.release() }
    accessibilityLeases.removeAll()
  }
  private static func presenter(in window: UIWindow) -> UIViewController? {
    guard var parent = window.rootViewController else { return nil }
    while let presented = parent.presentedViewController, !presented.isBeingDismissed { parent = presented }
    parent.loadViewIfNeeded()
    guard !parent.isBeingDismissed, parent.view.window === window else { return nil }
    return parent
  }
}

@MainActor
private final class AttachmentPresentationController: UIViewController, ZoenPresentationOverlayController {
  let host = AttachmentPresentationView()
  var hidesStatusBar = false
  override var prefersStatusBarHidden: Bool { hidesStatusBar }
  init(content: UIView) {
    super.init(nibName: nil, bundle: nil)
    host.backgroundColor = .clear
    host.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    host.accessibilityIdentifier = "attachment-presentation-host"
    content.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    host.addSubview(content)
    view = host
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
}

@MainActor
private final class AttachmentPresentationView: UIView {
  var onWindowLoss: (() -> Void)?
  private var hadWindow = false
  override func didMoveToWindow() {
    super.didMoveToWindow()
    if window != nil { hadWindow = true }
    else if hadWindow { onWindowLoss?() }
  }
}
