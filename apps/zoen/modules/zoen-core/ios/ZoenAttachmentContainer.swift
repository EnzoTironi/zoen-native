import UIKit

// Adapted from TelegramMessenger/Telegram-iOS, GPL-2.0-or-later (GPLv3 selected),
// revision f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838:
// AttachmentUI/Sources/AttachmentContainer.swift:16–28, 247–477, 504–665.
// Fixed viewport/wrapper/clip transforms and additional bottom inset remain
// distinct, as in the source. DeviceMetrics.swift:58–70, 118–213, 231–315
// supplies the original screen profile recognition and corner measurements.
// Display/Source/ContainedViewLayoutTransition.swift:41–83 and
// Display/Source/CAAnimationUtils.swift:66–110, 173–205 provide the actual
// custom spring and fallback entrance timing, without private duration APIs.
// Caption focus: AttachmentController.swift:762–766 and
// UIKitRuntimeUtils/Source/UIKitRuntimeUtils/UIKitUtils.m:53–83 (iOS 26 spring).
// Flat gallery geometry/restoration: Display/Source/Navigation/
// NavigationModalContainer.swift:384–430, 494–500, 503–543.
// UIKit hosts replace ASDisplayNode/NavigationContainer; Zoen retains admission,
// Photos selection and durable media ownership. See THIRD_PARTY_NOTICES.md.


/// A native child controller owns the attachment responder/presentation chain.
/// No React view is reparented and removal remains the close-before-handoff barrier.
@MainActor
final class ZoenAttachmentContainer: UIView, UIGestureRecognizerDelegate {
  let dim = UIControl()
  let wrapping = AttachmentWrappingView()
  let sheet = UIView()
  let bottomClip = UIView()
  let navigation = UIView()
  let handle = UIView()
  let content: any ZoenAttachmentOverlayContent
  private let requestClose: () -> Void
  var animator: UIViewPropertyAnimator?
  var layerMotionViews: [UIView] = []
  var presentation: ZoenAttachmentPresentationOwner?
  var expanded = false
  var fullScreen = false
  private var expansionBeforeFullScreen: Bool?
  private var closing = false
  var presented = false
  private var dismissalCompletion: (() -> Void)?
  var keyboardHeight: CGFloat = 0
  var sheetY: CGFloat = 0
  private var previousSize = CGSize.zero
  struct CornerProfileKey: Equatable {
    let size: CGSize
    let scale: CGFloat
    let statusBarHeight: CGFloat
    let homeIndicator: Bool
  }
  var cornerProfile: (key: CornerProfileKey, radius: CGFloat)?
  private var panState: (top: CGFloat, offset: CGFloat, scroll: UIScrollView?, bounces: Bool)?
  private lazy var pan = UIPanGestureRecognizer(target: self, action: #selector(panGesture(_:)))
  static let animationPrefix = "zoen.attachment.motion."

  struct VisualState {
    let view: UIView
    let position: CGPoint
    let bounds: CGRect
    let opacity: Float
    let transform: CATransform3D
    init(_ view: UIView) {
      self.view = view
      let layer = view.layer.presentation() ?? view.layer
      position = layer.position; bounds = layer.bounds; opacity = layer.opacity
      transform = layer.transform
    }
    func restore() {
      view.layer.position = position; view.layer.bounds = bounds; view.layer.opacity = opacity
      view.layer.transform = transform
    }
  }

  enum LayerCurve {
    case entrance
    case captionExpansion
    case settlement(initialVelocity: CGFloat)
  }

  init(content: any ZoenAttachmentOverlayContent, appearance: ZoenKeyboardPanelAppearance, requestClose: @escaping () -> Void) {
    self.content = content
    self.requestClose = requestClose
    super.init(frame: .zero)
    autoresizingMask = [.flexibleWidth, .flexibleHeight]
    accessibilityIdentifier = "attachment-container"
    accessibilityViewIsModal = true
    dim.backgroundColor = UIColor(white: 0, alpha: 0.25)
    dim.addAction(UIAction { [weak self] _ in self?.requestClose() }, for: .touchUpInside)
    dim.accessibilityLabel = ""
    dim.isAccessibilityElement = false
    addSubview(dim)
    wrapping.accessibilityIdentifier = "attachment-container-wrapping"
    sheet.accessibilityIdentifier = "attachment-container-sheet"
    sheet.layer.cornerCurve = .continuous
    sheet.clipsToBounds = true
    bottomClip.accessibilityIdentifier = "attachment-container-bottom-clip"
    navigation.accessibilityIdentifier = "attachment-container-navigation"
    bottomClip.clipsToBounds = true; bottomClip.layer.cornerCurve = .continuous
    navigation.clipsToBounds = true; navigation.backgroundColor = appearance.background
    addSubview(wrapping); wrapping.addSubview(sheet); sheet.addSubview(bottomClip)
    bottomClip.addSubview(navigation); navigation.addSubview(content.view)
    handle.backgroundColor = appearance.text.withAlphaComponent(appearance.isDark ? 0.2 : 0.07)
    handle.layer.cornerRadius = 2.5
    handle.isUserInteractionEnabled = false
    sheet.addSubview(handle)
    pan.delegate = self
    pan.delaysTouchesBegan = false
    pan.cancelsTouchesInView = true
    wrapping.addGestureRecognizer(pan)
    content.attachmentDidBeginEditing = { [weak self] in self?.expandForTextEntry() }
    content.attachmentPresentationChanged = { [weak self] in self?.contentPresentationChanged() }
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }

  var landscape: Bool { bounds.width > bounds.height }
  var inset: CGFloat {
    ZoenAttachmentContainerMetrics.collapsedInset(size: bounds.size, regular: traitCollection.horizontalSizeClass == .regular)
  }
  var baseTop: CGFloat {
    if landscape || fullScreen || traitCollection.horizontalSizeClass == .regular { return 0 }
    let systemHeight = window?.windowScene?.statusBarManager?.statusBarFrame.height ?? 0
    return 10 + (systemHeight > 0 ? systemHeight : safeAreaInsets.top)
  }
  private var restingY: CGFloat { baseTop + (expanded || fullScreen || traitCollection.horizontalSizeClass == .regular ? 0 : inset) }

  func present(in window: UIWindow) {
    guard !presented, !closing else { return }
    fullScreen = content.attachmentPrefersFullScreen
    if fullScreen { expansionBeforeFullScreen = expanded; expanded = true }
    guard let presentation = ZoenAttachmentPresentationOwner(content: self, in: window,
      onWindowLoss: { [weak self] in self?.requestClose() }) else { requestClose(); return }
    self.presentation = presentation
    presented = true
    layoutIfNeeded()
    sheetY = restingY
    applyLayout(updateDim: false)
    sheet.layoutIfNeeded()
    content.view.layoutIfNeeded()
    if fullScreen {
      // NavigationModalContainer's flat presentation adds no second modal
      // animation; the gallery owns the source-image transition.
      dim.alpha = 1
      UIAccessibility.post(notification: .screenChanged, argument: content.view)
      return
    }
    // Entrance translates the already laid-out panel as one object. Starting
    // with a zero-height sheet would accordion the gallery and floating bar.
    var entranceFrame = wrapping.frame
    entranceFrame.origin.y = bounds.height
    wrapping.frame = entranceFrame
    sheetY = baseTop + bounds.height
    dim.alpha = 0
    // AttachmentController.animateIn's path without a native glass source button:
    // 0.4-second .spring, represented by its pinned CAAnimationUtils Bézier.
    // The distinct glass-button morph requires that source button's real frame.
    animateLayers(to: restingY, duration: 0.4, curve: .entrance)
    UIAccessibility.post(notification: .screenChanged, argument: content.view)
  }

  func updateAppearance(_ appearance: ZoenKeyboardPanelAppearance) {
    navigation.backgroundColor = appearance.background
    handle.backgroundColor = appearance.text.withAlphaComponent(appearance.isDark ? 0.2 : 0.07)
  }

  private func expandForTextEntry() {
    guard !closing, !expanded else { return }
    expanded = true
    cancelPan()
    // Hardware keyboards produce no positive keyboard frame. Focus itself must
    // still expand the attachment, as AttachmentPanel.beganTextEditing requests.
    animateLayers(to: restingY, duration: 0.5, curve: .captionExpansion)
  }

  private func contentPresentationChanged() {
    guard !closing, presented else { return }
    cancelPan()
    let nextFullScreen = content.attachmentPrefersFullScreen
    if nextFullScreen || fullScreen {
      stopAnimation()
      if nextFullScreen && !fullScreen { expansionBeforeFullScreen = expanded; expanded = true }
      else if !nextFullScreen { expanded = expansionBeforeFullScreen ?? expanded; expansionBeforeFullScreen = nil }
      fullScreen = nextFullScreen
      sheetY = restingY
      // NavigationModalContainer.swift:384–430, 494–500, 503–543: flat
      // children occupy the full layout, without sheet chrome/entrance motion.
      applyLayout()
      return
    }
    expanded = true
    animateLayers(to: restingY, duration: 0.5, curve: .captionExpansion)
  }

  func keyboardChanged(height: CGFloat, duration: Double, curve: UIView.AnimationCurve) {
    guard !closing else { return }
    let next = max(0, height)
    guard abs(keyboardHeight - next) > 0.5 else { return }
    keyboardHeight = next
    if next > 0 { expanded = true }
    cancelPan()
    pan.isEnabled = next == 0
    stopAnimation()
    let animator = UIViewPropertyAnimator(duration: UIAccessibility.isReduceMotionEnabled ? 0 : duration, curve: curve) { [weak self] in
      guard let self else { return }
      self.sheetY = self.restingY
      self.applyLayout()
    }
    start(animator)
  }

  func dismiss(animated: Bool, completion: @escaping () -> Void) {
    guard !closing else { return }
    cancelPan()
    stopAnimation()
    closing = true
    dismissalCompletion = completion
    pan.isEnabled = false
    isUserInteractionEnabled = false
    endEditing(true)
    let complete: () -> Void = { [weak self] in self?.finishDismissal() }
    guard animated, window != nil, !UIAccessibility.isReduceMotionEnabled else { complete(); return }
    let animator = UIViewPropertyAnimator(duration: 0.25, curve: .easeInOut) { [weak self] in
      guard let self else { return }
      self.sheetY = self.baseTop + self.bounds.height
      self.dim.alpha = 0
      var frame = self.wrapping.frame
      frame.origin.y = self.bounds.height
      self.wrapping.frame = frame
    }
    animator.addCompletion { _ in complete() }
    start(animator)
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    guard bounds.size != previousSize else { return }
    if closing { finishDismissal(); return }
    if previousSize != .zero {
      stopAnimation()
      cancelPan()
    }
    previousSize = bounds.size
    if landscape { expanded = true }
    sheetY = restingY
    applyLayout()
  }

  private func removePresentation() {
    presentation?.remove()
    presentation = nil
    removeFromSuperview()
  }

  private func finishDismissal() {
    guard let completion = dismissalCompletion else { return }
    dismissalCompletion = nil
    if animator?.state == .active { animator?.stopAnimation(true) }
    animator = nil
    removeLayerMotion()
    removePresentation()
    completion()
  }


  private func cancelPan() {
    if let state = panState { state.scroll?.bounces = state.bounces }
    panState = nil
  }

  override func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
    guard !closing, keyboardHeight == 0, !content.attachmentHandlesVerticalGesture,
          traitCollection.horizontalSizeClass != .regular else { return false }
    let velocity = pan.velocity(in: self)
    return abs(velocity.y) > abs(velocity.x)
  }
  func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
    var view = touch.view
    while let current = view, current !== wrapping {
      if current is UITextView || current is UIControl { return false }
      view = current.superview
    }
    return true
  }
  func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer) -> Bool {
    otherGestureRecognizer === content.attachmentScrollView.panGestureRecognizer
  }
  @objc private func panGesture(_ recognizer: UIPanGestureRecognizer) {
    switch recognizer.state {
    case .began:
      stopAnimation()
      let point = recognizer.location(in: content.attachmentScrollView)
      let scroll = content.attachmentScrollView.bounds.contains(point) ? content.attachmentScrollView : nil
      panState = (expanded ? 0 : inset, 0, scroll, scroll?.bounces ?? true)
    case .changed:
      guard var state = panState else { return }
      var translation = recognizer.translation(in: self).y
      var offset = state.top + translation
      if let scroll = state.scroll {
        if scroll.contentOffset.y <= -scroll.contentInset.top + 1 {
          scroll.bounces = false
          scroll.setContentOffset(CGPoint(x: scroll.contentOffset.x, y: -scroll.contentInset.top), animated: false)
        } else {
          translation = state.offset
          offset = state.top + translation
          if expanded { recognizer.setTranslation(.zero, in: self) }
          else if offset > 0 { scroll.setContentOffset(CGPoint(x: scroll.contentOffset.x, y: -scroll.contentInset.top), animated: false) }
        }
        if !expanded && offset > 0 { scroll.panGestureRecognizer.setTranslation(.zero, in: scroll) }
      }
      state.offset = translation
      panState = state
      sheetY = baseTop + max(0, offset)
      applyLayout()
    case .ended:
      guard let state = panState else { return }
      let velocity = recognizer.velocity(in: self).y
      let translation = recognizer.translation(in: self).y
      let innerOffset = state.scroll.map { $0.contentOffset.y + $0.contentInset.top } ?? 0
      let destination = ZoenAttachmentContainerMetrics.destination(expanded: expanded, inset: inset, offset: state.top + state.offset, translation: translation, velocity: velocity, innerOffset: innerOffset)
      cancelPan()
      if destination == .dismissed { requestClose(); return }
      let wasExpanded = expanded
      expanded = destination == .expanded
      if !expanded, let scroll = state.scroll { scroll.setContentOffset(CGPoint(x: scroll.contentOffset.x, y: -scroll.contentInset.top), animated: false) }
      animate(to: restingY, duration: expanded == wasExpanded ? 0.3 : 0.45, spring: expanded != wasExpanded, velocity: velocity)
    case .cancelled, .failed:
      cancelPan()
      animate(to: restingY, duration: 0.3, spring: false, velocity: 0)
    default: break
    }
  }
  override func accessibilityPerformEscape() -> Bool { requestClose(); return true }
}


@MainActor
final class AttachmentWrappingView: UIView {
  override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
    let result = super.hitTest(point, with: event)
    return result === self ? nil : result
  }
}
