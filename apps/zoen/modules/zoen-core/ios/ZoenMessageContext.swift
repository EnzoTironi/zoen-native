import ExpoModulesCore
import UIKit

// Original UIKit implementation of the pinned Telegram interaction contract.
// The Stage stays under the original Fabric + RNGH root. No portal, snapshot,
// second renderer/player, private UIKit API, or alternate message authority.
@MainActor
final class ZoenMessageContextStage: ExpoView {
  let contentHost = UIView()
  let overlayHost = UIView()
  private var children: [UIView] = []
  lazy var contextGesture = ZoenMessageContextGesture()
  private(set) var controller: ZoenMessageContextController?
  var requestedActive = true
  private(set) var active = true

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    addSubview(contentHost)
    addSubview(overlayHost)
    addGestureRecognizer(contextGesture)
    overlayHost.isHidden = true
    clipsToBounds = false
  }
  func bind(active: Bool) {
    self.active = active
    if !active { contextGesture.cancelAdmission(); controller?.retire() }
  }
  override func mountChildComponentView(_ childComponentView: UIView, index: Int) {
    guard index >= 0, index <= children.count, childComponentView.superview == nil else { return }
    controller?.retire()
    children.insert(childComponentView, at: index)
    contentHost.insertSubview(childComponentView, at: index)
  }
  override func unmountChildComponentView(_ childComponentView: UIView, index: Int) {
    controller?.retire()
    guard children.indices.contains(index), children[index] === childComponentView else { return }
    children.remove(at: index)
    childComponentView.removeFromSuperview()
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    contentHost.frame = bounds
    overlayHost.frame = bounds
    controller?.view.frame = overlayHost.bounds
    controller?.reflow()
  }
  override func didMoveToWindow() {
    super.didMoveToWindow()
    if window == nil { controller?.retire() }
  }
  func dismissContexts() async { if let controller { await controller.close() } }
  func retireContexts() async {
    active = false
    contextGesture.cancelAdmission()
    if let controller { controller.retire(); await controller.close() }
  }
  func admit(_ controller: ZoenMessageContextController) throws {
    guard active, self.controller == nil, let window,
          window.windowScene?.activationState == .foregroundActive,
          let parent = owningController, !parent.isBeingDismissed,
          !parent.isBeingPresented, parent.presentedViewController == nil else {
      throw ZoenMessageContextSource.Failure.unavailable
    }
    try controller.admit(stage: self)
    self.controller = controller
    overlayHost.isHidden = false
    contentHost.accessibilityElementsHidden = true
    parent.addChild(controller)
    controller.beginAppearanceTransition(true, animated: false)
    overlayHost.addSubview(controller.view)
    controller.view.frame = overlayHost.bounds
    controller.didMove(toParent: parent)
    controller.endAppearanceTransition()
    controller.extract()
  }
  func remove(_ controller: ZoenMessageContextController) {
    guard self.controller === controller else { return }
    // Restore first, remove the actual contained controller second. Completion
    // is published by the controller only AFTER this synchronous barrier.
    controller.restoreSource()
    controller.resignFirstResponder()
    controller.willMove(toParent: nil)
    controller.beginAppearanceTransition(false, animated: false)
    controller.view.removeFromSuperview()
    controller.removeFromParent()
    controller.endAppearanceTransition()
    self.controller = nil
    overlayHost.isHidden = true
    contentHost.accessibilityElementsHidden = false
  }
  private var owningController: UIViewController? {
    var responder: UIResponder? = self
    while let current = responder {
      if let controller = current as? UIViewController { return controller }
      responder = current.next
    }
    return nil
  }
}

@MainActor
final class ZoenMessageContextSource: ExpoView {
  enum Failure: Error { case unavailable, changed, busy, configuration }
  let onGesture = EventDispatcher()
  let contentHost = UIView()
  private(set) var children: [UIView] = []
  private(set) var generation: UInt64 = 0
  private(set) var binding = ""
  private(set) var active = false
  private(set) var presentation: ZoenMessageContextController?
  private var cancelledRequests: [UUID] = []
  private var lastGesture: UUID?
  private weak var lastGestureText: UITextView?
  private var lastGestureHadText = false
  private var lastGestureTextPoint = CGPoint.zero
  private var lastGestureTextGeneration: UInt64 = 0
  private var gestureSequence = 0
  var requestedBinding = ""
  var requestedActive = false
  var requestedHandlesTap = false
  var requestedReducedMotion = false
  private(set) var handlesTap = false
  private(set) var reducedMotion = false

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    clipsToBounds = false
    contentHost.clipsToBounds = false
    addSubview(contentHost)
    NotificationCenter.default.addObserver(self, selector: #selector(background),
      name: UIApplication.willResignActiveNotification, object: nil)
  }
  func bind(_ next: String, active: Bool, handlesTap: Bool, reducedMotion: Bool = false) {
    guard UUID(uuidString: next) != nil else { retire(); binding = ""; self.active = false; return }
    if next != binding || !active { retire(); binding = next }
    self.active = active
    self.handlesTap = handlesTap
    self.reducedMotion = reducedMotion
    if reducedMotion { resetPress(animated: false) }
  }
  override func mountChildComponentView(_ childComponentView: UIView, index: Int) {
    retire()
    guard childComponentView.superview == nil, index >= 0, index <= children.count else { return }
    children.insert(childComponentView, at: index)
    contentHost.insertSubview(childComponentView, at: index)
    setNeedsLayout()
  }
  override func unmountChildComponentView(_ childComponentView: UIView, index: Int) {
    retire()
    guard children.indices.contains(index), children[index] === childComponentView,
          childComponentView.superview === contentHost else { return }
    children.remove(at: index)
    childComponentView.removeFromSuperview()
    setNeedsLayout()
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    if let presentation { presentation.reflow() } else { contentHost.frame = bounds }
  }
  override func didMoveToWindow() {
    super.didMoveToWindow()
    if window == nil { retire() }
  }
  @objc private func background() { retire() }
  func retire() {
    generation &+= 1
    lastGesture = nil
    lastGestureText = nil
    lastGestureHadText = false
    lastGestureTextPoint = .zero
    if nearestStage?.contextGesture.source === self { nearestStage?.contextGesture.cancelAdmission() }
    presentation?.retire()
    resetPress(animated: false)
  }
  var gestureAdmitted: Bool {
    active && presentation == nil && children.count == 1 && window != nil &&
      window?.windowScene?.activationState == .foregroundActive && nearestStage?.active == true && nearestStage?.controller == nil
  }
  func dispatch(kind: String, textView: UITextView? = nil, textPoint: CGPoint = .zero, hadText: Bool = false) {
    guard gestureAdmitted else { return }
    let gesture = UUID()
    lastGesture = gesture
    lastGestureText = textView.flatMap { $0.isDescendant(of: contentHost) ? $0 : nil }
    lastGestureHadText = hadText || textView != nil
    lastGestureTextPoint = textPoint
    lastGestureTextGeneration = generation
    gestureSequence += 1
    onGesture(["binding": binding, "gestureId": gesture.uuidString, "sequence": gestureSequence, "kind": kind])
  }
  func press(_ progress: CGFloat) {
    guard presentation == nil, bounds.width > 0 else { return }
    if reducedMotion || UIAccessibility.isReduceMotionEnabled { resetPress(animated: false); return }
    let target = max(0.7, (bounds.width - 15) / bounds.width)
    contentHost.transform = CGAffineTransform(scaleX: 1 - (1 - target) * progress,
                                             y: 1 - (1 - target) * progress)
  }
  func resetPress(animated: Bool) {
    guard presentation == nil else { return }
    if animated && !reducedMotion && !UIAccessibility.isReduceMotionEnabled {
      UIView.animate(withDuration: 0.2, delay: 0, options: [.beginFromCurrentState, .curveEaseOut]) {
        self.contentHost.transform = .identity
      }
    } else { contentHost.layer.removeAllAnimations(); contentHost.transform = .identity }
  }
  func cancelSurfaceTouch() {
    // Expo's public LinkPreview uses this cancellation seam. Never call reset
    // directly while UIKit still owns a touch registered in the Fabric handler.
    guard let touchHandlerClass = NSClassFromString("RCTSurfaceTouchHandler") else { return }
    var node: UIView? = self
    while let current = node {
      if let handler = current.gestureRecognizers?.first(where: { $0.isKind(of: touchHandlerClass) }) {
        handler.isEnabled = false
        handler.isEnabled = true
        break
      }
      node = current.superview
    }
  }
  var nearestStage: ZoenMessageContextStage? {
    var node: UIView? = superview
    while let current = node {
      if let stage = current as? ZoenMessageContextStage { return stage }
      node = current.superview
    }
    return nil
  }
  func consumeGesture(expectedBinding: String, gestureId: String) -> Bool {
    guard active, binding == expectedBinding, lastGesture?.uuidString == gestureId,
          gestureAdmitted, let window, visible(in: window) else { return false }
    lastGesture = nil
    lastGestureText = nil
    lastGestureHadText = false
    lastGestureTextPoint = .zero
    return true
  }
  func present(requestId: String, expectedBinding: String, gestureId: String, configuration: String) async throws -> String? {
    try Task.checkCancellation()
    guard let request = UUID(uuidString: requestId), UUID(uuidString: expectedBinding) != nil,
          !cancelledRequests.contains(request) else { throw Failure.changed }
    guard presentation == nil else { throw Failure.busy }
    if !gestureId.isEmpty && lastGesture?.uuidString != gestureId { return nil }
    guard active, binding == expectedBinding, children.count == 1,
          let stage = nearestStage, let window,
          window.windowScene?.activationState == .foregroundActive,
          visible(in: window) else { throw Failure.unavailable }
    let config = try ZoenMessageContextConfiguration.decode(configuration)
    let controller = ZoenMessageContextController(source: self, configuration: config,
      request: request, generation: generation, binding: binding, child: children[0], gestureId: gestureId, heldText: !gestureId.isEmpty && lastGestureTextGeneration == generation ? lastGestureText : nil, heldTextPoint: lastGestureTextPoint, hadHeldText: !gestureId.isEmpty && lastGestureTextGeneration == generation && lastGestureHadText)
    presentation = controller
    do {
      let result: String? = try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { continuation in
          controller.complete = { continuation.resume(with: $0) }
          do { try stage.admit(controller) }
          catch { controller.failed(error) }
        }
      } onCancel: {
        Task { @MainActor [weak self] in await self?.dismiss(requestId: requestId) }
      }
      try Task.checkCancellation()
      return result
    } catch {
      if presentation === controller { controller.retire() }
      throw error
    }
  }
  func selectionPhase(_ controller: ZoenMessageContextController, active: Bool) {
    guard presentation === controller else { return }
    gestureSequence += 1
    onGesture(["binding": controller.binding, "gestureId": controller.gestureId,
      "sequence": gestureSequence, "kind": "selection",
      "selection": ["requestId": controller.request.uuidString, "active": active]])
  }
  func finishTextSelection(requestId: String, resultJSON: String?) async throws {
    guard let request = UUID(uuidString: requestId), let presentation,
          presentation.request == request, presentation.isCurrent,
          presentation.selectingText, !presentation.closing else { throw Failure.changed }
    if let resultJSON {
      guard let data = resultJSON.data(using: .utf8), data.count <= 65_536,
            (try? JSONSerialization.jsonObject(with: data)) is [String: Any] else { throw Failure.configuration }
    }
    await presentation.close(selectionResult: resultJSON.map { "select:" + $0 })
  }
  func dismiss(requestId: String) async {
    guard let request = UUID(uuidString: requestId) else { return }
    if !cancelledRequests.contains(request) {
      cancelledRequests.append(request)
      if cancelledRequests.count > 64 { cancelledRequests.removeFirst() }
    }
    guard let presentation, presentation.request == request else { return }
    await presentation.close()
  }
  func current(_ controller: ZoenMessageContextController) -> Bool {
    active && presentation === controller && generation == controller.sourceGeneration &&
      binding == controller.binding && children.count == 1 && children.first === controller.child &&
      window === controller.sourceWindow && window?.windowScene?.activationState == .foregroundActive
  }
  func restore(_ controller: ZoenMessageContextController) {
    guard presentation === controller else { return }
    contentHost.layer.removeAllAnimations()
    contentHost.transform = .identity
    contentHost.removeFromSuperview()
    addSubview(contentHost)
    contentHost.frame = bounds
  }
  func finished(_ controller: ZoenMessageContextController) {
    guard presentation === controller else { return }
    restore(controller)
    presentation = nil
    lastGesture = nil
    lastGestureText = nil
    lastGestureHadText = false
    lastGestureTextPoint = .zero
  }
  private func visible(in window: UIWindow) -> Bool {
    var node: UIView? = self
    while let current = node {
      if current.isHidden || current.alpha <= 0.01 { return false }
      node = current.superview
    }
    let frame = convert(bounds, to: window)
    return frame.contextFinite && frame.intersects(window.bounds)
  }
}
