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
struct ZoenAttachmentContainerMetrics {
  enum Destination: Equatable { case expanded, collapsed, dismissed }
  struct Layout {
    let wrapping: CGRect
    let clip: CGRect
    let clipScale: CGFloat
    let bottomClip: CGRect
    let navigation: CGRect
    let additionalBottomInset: CGFloat
  }

  // AttachmentContainer.swift:530–665. The controller viewport stays full
  // height; the wrapper/clip and additional bottom inset expose the sheet.
  static func layout(size: CGSize, baseTop: CGFloat, offset: CGFloat, inset: CGFloat,
                     landscape: Bool, regular: Bool, fullScreen: Bool, scale: CGFloat) -> Layout {
    let rawOffset = fullScreen || regular ? 0 : max(0, offset)
    let topInset = landscape ? 0 : min(inset, rawOffset)
    let scaleProgress: CGFloat = fullScreen || regular ? 0 : (landscape ? 1 : (inset == 0 ? 0 : topInset / inset))
    let viewportHeight = max(0, size.height - baseTop)
    let clipTopOffset: CGFloat = fullScreen || regular || landscape ? 0 : 2 * scaleProgress + 1 / max(1, scale)
    let clip = CGRect(x: 0, y: baseTop + clipTopOffset, width: size.width, height: max(0, viewportHeight - topInset))
    return Layout(wrapping: CGRect(x: 0, y: rawOffset, width: size.width, height: size.height),
      clip: clip, clipScale: size.width > 0 ? 1 - 10 / size.width * scaleProgress : 1,
      bottomClip: CGRect(x: 0, y: -100, width: size.width, height: clip.height + 100),
      navigation: CGRect(x: 0, y: 100, width: size.width, height: viewportHeight),
      additionalBottomInset: topInset)
  }

  static func collapsedInset(size: CGSize, regular: Bool) -> CGFloat {
    guard size.width <= size.height else { return 0 }
    if regular { return 210 }
    return floor(max(size.width, size.height) * (size.width <= 320 ? 0.15 : 0.2488))
  }

  static func destination(expanded: Bool, inset: CGFloat, offset: CGFloat,
                          translation: CGFloat, velocity: CGFloat,
                          innerOffset: CGFloat) -> Destination {
    let velocity = expanded && innerOffset > 0.1 ? 0 : velocity
    let boundsY = -max(0, expanded ? translation - inset : translation)
    if boundsY < -60 || (boundsY < 0 && velocity > 300) || (expanded && boundsY == 0 && velocity > 1800) {
      return .dismissed
    }
    if expanded { return velocity > 300 || offset > inset / 2 ? .collapsed : .expanded }
    return velocity < -300 || offset < inset / 2 ? .expanded : .collapsed
  }
}

@MainActor
protocol ZoenAttachmentOverlayContent: ZoenKeyboardPanelContent {
  var attachmentScrollView: UIScrollView { get }
  var attachmentHasKeyboardFocus: Bool { get }
  var attachmentHandlesVerticalGesture: Bool { get }
  var attachmentPrefersFullScreen: Bool { get }
  var attachmentAnimatedViews: [UIView] { get }
  var attachmentDidBeginEditing: (() -> Void)? { get set }
  var attachmentPresentationChanged: (() -> Void)? { get set }
  func updatePresentation(expansion: CGFloat, keyboardHeight: CGFloat, safeArea: UIEdgeInsets)
  func updatePresentation(expansion: CGFloat, keyboardHeight: CGFloat, safeArea: UIEdgeInsets, additionalBottomInset: CGFloat)
  func retireForDismissal(completion: @escaping @MainActor () -> Void)
}

extension ZoenAttachmentOverlayContent {
  var attachmentHandlesVerticalGesture: Bool { false }
  var attachmentPrefersFullScreen: Bool { false }
  var attachmentPresentationChanged: (() -> Void)? { get { nil } set {} }
  func updatePresentation(expansion: CGFloat, keyboardHeight: CGFloat, safeArea: UIEdgeInsets, additionalBottomInset: CGFloat) {
    updatePresentation(expansion: expansion, keyboardHeight: keyboardHeight, safeArea: safeArea)
  }
}

/// Hide only the underlying root's accessibility subtree while an admitted
/// window-sibling attachment is present. Overlapping owners restore the exact
/// original value only after the last removal; no composer or draft is mutated.
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

/// The attachment owner is a window sibling, never a reparented React view or a
/// second inputView. Removal completion is the close-before-handoff barrier.
@MainActor
final class ZoenAttachmentContainer: UIView, UIGestureRecognizerDelegate {
  private let dim = UIControl()
  private let wrapping = AttachmentWrappingView()
  private let sheet = UIView()
  private let bottomClip = UIView()
  private let navigation = UIView()
  private let handle = UIView()
  private let content: any ZoenAttachmentOverlayContent
  private let requestClose: () -> Void
  private var animator: UIViewPropertyAnimator?
  private var layerMotionViews: [UIView] = []
  private var accessibilityLease: ZoenAttachmentAccessibilityLease?
  private var expanded = false
  private var fullScreen = false
  private var expansionBeforeFullScreen: Bool?
  private var closing = false
  private var presented = false
  private var dismissalCompletion: (() -> Void)?
  private var keyboardHeight: CGFloat = 0
  private var sheetY: CGFloat = 0
  private var previousSize = CGSize.zero
  private struct CornerProfileKey: Equatable {
    let size: CGSize
    let scale: CGFloat
    let statusBarHeight: CGFloat
    let homeIndicator: Bool
  }
  private var cornerProfile: (key: CornerProfileKey, radius: CGFloat)?
  private var panState: (top: CGFloat, offset: CGFloat, scroll: UIScrollView?, bounces: Bool)?
  private lazy var pan = UIPanGestureRecognizer(target: self, action: #selector(panGesture(_:)))
  private static let animationPrefix = "zoen.attachment.motion."

  private struct VisualState {
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

  private enum LayerCurve {
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

  private var landscape: Bool { bounds.width > bounds.height }
  private var inset: CGFloat {
    ZoenAttachmentContainerMetrics.collapsedInset(size: bounds.size, regular: traitCollection.horizontalSizeClass == .regular)
  }
  private var baseTop: CGFloat {
    if landscape || fullScreen || traitCollection.horizontalSizeClass == .regular { return 0 }
    let systemHeight = window?.windowScene?.statusBarManager?.statusBarFrame.height ?? 0
    return 10 + (systemHeight > 0 ? systemHeight : safeAreaInsets.top)
  }
  private var restingY: CGFloat { baseTop + (expanded || fullScreen || traitCollection.horizontalSizeClass == .regular ? 0 : inset) }

  func present(in window: UIWindow) {
    guard !presented, !closing else { return }
    presented = true
    fullScreen = content.attachmentPrefersFullScreen
    if fullScreen { expansionBeforeFullScreen = expanded; expanded = true }
    frame = window.bounds
    window.addSubview(self)
    if let root = window.rootViewController?.view, !isDescendant(of: root) {
      accessibilityLease = ZoenAttachmentAccessibilityLease(root: root)
    }
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

  override func didMoveToWindow() {
    super.didMoveToWindow()
    if window == nil {
      accessibilityLease?.release()
      accessibilityLease = nil
    }
  }

  private func finishDismissal() {
    guard let completion = dismissalCompletion else { return }
    dismissalCompletion = nil
    if animator?.state == .active { animator?.stopAnimation(true) }
    animator = nil
    removeLayerMotion()
    removeFromSuperview()
    accessibilityLease?.release()
    accessibilityLease = nil
    completion()
  }

  private func applyLayout(updateDim: Bool = true) {
    dim.frame = bounds
    let regular = traitCollection.horizontalSizeClass == .regular
    let layout = ZoenAttachmentContainerMetrics.layout(size: bounds.size, baseTop: baseTop,
      offset: sheetY - baseTop, inset: inset, landscape: landscape, regular: regular,
      fullScreen: fullScreen, scale: traitCollection.displayScale)
    wrapping.frame = layout.wrapping
    sheet.bounds = CGRect(origin: .zero, size: layout.clip.size)
    sheet.center = CGPoint(x: layout.clip.midX, y: layout.clip.midY)
    sheet.layer.transform = CATransform3DMakeScale(layout.clipScale, layout.clipScale, 1)
    sheet.layer.cornerRadius = landscape || fullScreen || regular ? 0 : 38
    sheet.layer.maskedCorners = [.layerMinXMinYCorner, .layerMaxXMinYCorner, .layerMinXMaxYCorner, .layerMaxXMaxYCorner]
    bottomClip.frame = layout.bottomClip
    let profileKey = CornerProfileKey(size: window?.screen.bounds.size ?? bounds.size,
      scale: traitCollection.displayScale, statusBarHeight: safeAreaInsets.top, homeIndicator: safeAreaInsets.bottom > 0)
    if cornerProfile?.key != profileKey {
      cornerProfile = (profileKey, ZoenAttachmentScreenCornerRadius(size: profileKey.size, scale: profileKey.scale,
        statusBarHeight: profileKey.statusBarHeight, homeIndicator: profileKey.homeIndicator))
    }
    bottomClip.layer.cornerRadius = fullScreen ? 0 : max(24, cornerProfile?.radius ?? 0) - 2
    navigation.frame = layout.navigation
    content.view.frame = navigation.bounds
    let progress = inset == 0 ? 1 : 1 - min(1, max(0, (sheetY - baseTop) / inset))
    content.updatePresentation(expansion: progress, keyboardHeight: keyboardHeight, safeArea: safeAreaInsets,
      additionalBottomInset: layout.additionalBottomInset)
    handle.frame = CGRect(x: floor((sheet.bounds.width - 36) / 2), y: 5, width: 36, height: 5)
    handle.isHidden = fullScreen || traitCollection.horizontalSizeClass == .regular
    dim.backgroundColor = fullScreen ? .clear : UIColor(white: 0, alpha: 0.25)
    if updateDim { dim.alpha = 1 }
  }

  private func stopAnimation() {
    guard animator != nil || !layerMotionViews.isEmpty else { return }
    let states = motionViews().map(VisualState.init)
    if let frame = wrapping.layer.presentation()?.frame { sheetY = baseTop + frame.minY }
    if animator?.state == .active { animator?.stopAnimation(true) }
    self.animator = nil
    removeLayerMotion()
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    applyLayout(updateDim: false)
    // Retarget from the displayed positions/bounds, including the floating bar,
    // rather than jumping children to a previous animation's model end values.
    for state in states { state.restore() }
    CATransaction.commit()
  }

  private func animate(to y: CGFloat, duration: Double, spring: Bool, velocity: CGFloat) {
    if spring {
      stopAnimation()
      let distance = y - sheetY
      let initialVelocity = distance == 0 ? 0 : abs(velocity / distance)
      animateLayers(to: y, duration: duration, curve: .settlement(initialVelocity: initialVelocity))
      return
    }
    stopAnimation()
    let duration = UIAccessibility.isReduceMotionEnabled ? 0 : duration
    let animator = UIViewPropertyAnimator(duration: duration, curve: .easeInOut)
    animator.addAnimations { [weak self] in self?.sheetY = y; self?.applyLayout() }
    start(animator)
  }

  private func start(_ animator: UIViewPropertyAnimator) {
    self.animator = animator
    animator.addCompletion { [weak self, weak animator] _ in
      guard let self, let animator, self.animator === animator else { return }
      self.animator = nil
    }
    animator.startAnimation()
  }

  private func motionViews() -> [UIView] {
    var seen = Set<ObjectIdentifier>()
    return ([wrapping, sheet, bottomClip, navigation, dim, handle, content.view] + content.attachmentAnimatedViews)
      .filter { seen.insert(ObjectIdentifier($0)).inserted }
  }

  private func removeLayerMotion() {
    for view in layerMotionViews {
      for key in ["position", "bounds", "opacity", "transform"] {
        view.layer.removeAnimation(forKey: Self.animationPrefix + key)
      }
    }
    layerMotionViews.removeAll()
  }

  private func animateLayers(to y: CGFloat, duration: Double, curve: LayerCurve) {
    stopAnimation()
    let states = motionViews().map(VisualState.init)
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    sheetY = y
    applyLayout(updateDim: false)
    dim.alpha = 1
    sheet.layoutIfNeeded()
    content.view.layoutIfNeeded()
    CATransaction.commit()
    guard duration > 0, !UIAccessibility.isReduceMotionEnabled else { return }
    layerMotionViews = states.map(\.view)
    for state in states {
      let layer = state.view.layer
      if state.position != layer.position {
        addLayerMotion(layer: layer, from: NSValue(cgPoint: state.position), to: NSValue(cgPoint: layer.position), key: "position", duration: duration, curve: curve)
      }
      if state.bounds != layer.bounds {
        addLayerMotion(layer: layer, from: NSValue(cgRect: state.bounds), to: NSValue(cgRect: layer.bounds), key: "bounds", duration: duration, curve: curve)
      }
      if !CATransform3DEqualToTransform(state.transform, layer.transform) {
        addLayerMotion(layer: layer, from: NSValue(caTransform3D: state.transform), to: NSValue(caTransform3D: layer.transform), key: "transform", duration: duration, curve: curve)
      }
      if state.opacity != layer.opacity {
        if state.view === dim {
          let fade = CABasicAnimation(keyPath: "opacity")
          fade.fromValue = state.opacity; fade.toValue = layer.opacity
          fade.duration = 0.3; fade.timingFunction = CAMediaTimingFunction(name: .linear)
          layer.add(fade, forKey: Self.animationPrefix + "opacity")
        } else {
          addLayerMotion(layer: layer, from: state.opacity, to: layer.opacity, key: "opacity", duration: duration, curve: curve)
        }
      }
    }
  }

  private func addLayerMotion(layer: CALayer, from: Any, to: Any, key: String,
                              duration: Double, curve: LayerCurve) {
    let animation: CABasicAnimation
    switch curve {
    case .entrance:
      animation = CABasicAnimation(keyPath: key)
      animation.duration = duration
      animation.timingFunction = CAMediaTimingFunction(controlPoints: 0.380, 0.700, 0.125, 1.000)
    case .captionExpansion:
      let spring = CASpringAnimation(keyPath: key)
      spring.mass = 1; spring.stiffness = 555.027; spring.damping = 47.118
      spring.duration = duration
      spring.allowsOverdamping = false
      spring.timingFunction = CAMediaTimingFunction(name: .linear)
      animation = spring
    case let .settlement(initialVelocity):
      let spring = CASpringAnimation(keyPath: key)
      spring.mass = 5; spring.stiffness = 900; spring.damping = 124
      // Upstream serializes the parameters through Float in CAAnimationUtils.
      spring.initialVelocity = CGFloat(Float(initialVelocity))
      spring.duration = spring.settlingDuration
      spring.speed = Float(spring.duration / duration)
      spring.timingFunction = CAMediaTimingFunction(name: .linear)
      animation = spring
    }
    animation.fromValue = from; animation.toValue = to
    animation.isRemovedOnCompletion = true; animation.fillMode = .forwards
    let fps = Float(window?.screen.maximumFramesPerSecond ?? 60)
    if key != "opacity", fps > 61 {
      animation.preferredFrameRateRange = CAFrameRateRange(minimum: 30, maximum: fps, preferred: fps)
    }
    layer.add(animation, forKey: Self.animationPrefix + key)
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
private final class AttachmentWrappingView: UIView {
  override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
    let result = super.hitTest(point, with: event)
    return result === self ? nil : result
  }
}

// DeviceMetrics.swift:58–70, 118–213, 231–315. These are upstream profile
// recognition measurements, never a replacement for the window's layout size.
private func ZoenAttachmentScreenCornerRadius(size: CGSize, scale: CGFloat, statusBarHeight: CGFloat, homeIndicator: Bool) -> CGFloat {
  struct Profile {
    let size: CGSize
    let radius: CGFloat
    let models: [String]
    var deviceScale: CGFloat = 3
  }
  let pixel = 1 / max(1, scale)
  let profiles: [Profile] = [
    Profile(size: CGSize(width: 320, height: 480), radius: 0, models: []),
    Profile(size: CGSize(width: 320, height: 568), radius: 0, models: []),
    Profile(size: CGSize(width: 375, height: 667), radius: 0, models: []),
    Profile(size: CGSize(width: 414, height: 736), radius: 0, models: []),
    Profile(size: CGSize(width: 375, height: 812), radius: 39, models: ["iPhone10,3", "iPhone10,6", "iPhone11,2", "iPhone12,3"]),
    Profile(size: CGSize(width: 414, height: 896), radius: 39, models: ["iPhone11,4", "iPhone11,6", "iPhone12,5"]),
    Profile(size: CGSize(width: 414, height: 896), radius: 41.5, models: ["iPhone11,8", "iPhone12,1"], deviceScale: 2),
    Profile(size: CGSize(width: 375, height: 812), radius: 44, models: ["iPhone13,1"]),
    Profile(size: CGSize(width: 390, height: 844), radius: 47 + pixel, models: ["iPhone13,2", "iPhone13,3"]),
    Profile(size: CGSize(width: 428, height: 926), radius: 53 + pixel, models: ["iPhone13,4"]),
    Profile(size: CGSize(width: 375, height: 812), radius: 0, models: ["iPhone14,4"]),
    Profile(size: CGSize(width: 390, height: 844), radius: 47 + pixel, models: ["iPhone14,5", "iPhone14,7", "iPhone17,5", "iPhone18,5", "iPhone14,2"]),
    Profile(size: CGSize(width: 428, height: 926), radius: 53 + pixel, models: ["iPhone14,3", "iPhone14,8"]),
    Profile(size: CGSize(width: 393, height: 852), radius: 55, models: ["iPhone15,2", "iPhone15,4", "iPhone16,1", "iPhone17,3"]),
    Profile(size: CGSize(width: 320, height: 693), radius: 47 + pixel, models: ["iPhone15,2", "iPhone15,4", "iPhone16,1", "iPhone17,3"]),
    Profile(size: CGSize(width: 430, height: 932), radius: 55, models: ["iPhone15,3", "iPhone15,5", "iPhone16,2", "iPhone17,4"]),
    Profile(size: CGSize(width: 375, height: 812), radius: 53 + pixel, models: ["iPhone15,3", "iPhone15,5", "iPhone16,2", "iPhone17,4"]),
    Profile(size: CGSize(width: 402, height: 874), radius: 62, models: ["iPhone17,1", "iPhone18,1", "iPhone18,3", "iPhone19,2"]),
    Profile(size: CGSize(width: 440, height: 956), radius: 62, models: ["iPhone17,2", "iPhone18,2", "iPhone19,3", "iPhone19,7"]),
    Profile(size: CGSize(width: 420, height: 912), radius: 62, models: ["iPhone18,4"]),
    Profile(size: CGSize(width: 466, height: 678), radius: 0, models: ["iPhone19,4"])
  ]
  let portrait = CGSize(width: min(size.width, size.height), height: max(size.width, size.height))
  let extended = CGSize(width: portrait.width, height: portrait.height + 20)
  let model: String?
  #if targetEnvironment(simulator)
  model = ProcessInfo.processInfo.environment["SIMULATOR_MODEL_IDENTIFIER"]
  #else
  var info = utsname()
  model = uname(&info) == 0 ? withUnsafeBytes(of: &info.machine) { String(decoding: $0.prefix(while: { $0 != 0 }), as: UTF8.self) } : nil
  #endif
  if let model, let profile = profiles.first(where: { $0.models.contains(model) && $0.deviceScale == scale && ($0.size == portrait || $0.size == extended) }) { return profile.radius }
  if let profile = profiles.first(where: { ($0.size == portrait || $0.size == extended) && (!homeIndicator || $0.radius > 0) }) {
    if profile.size == CGSize(width: 375, height: 812), profile.radius == 39, statusBarHeight == 47 { return 53 + pixel }
    if profile.size == CGSize(width: 414, height: 896), profile.radius == 39, scale == 2 { return 41.5 }
    return profile.radius
  }
  if portrait.width >= 1024 || portrait.height >= 1024 { return 0 }
  return homeIndicator ? 39 : 0
}
