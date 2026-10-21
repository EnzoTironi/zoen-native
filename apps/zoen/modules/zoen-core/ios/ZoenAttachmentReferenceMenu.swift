import UIKit

// UIKit translation of the modern reference/list ContextController branch at
// Telegram-iOS f1dd7a2 (GPL-2.0-or-later, GPLv3 selected). Exact source ranges
// and adaptations: docs/product/telegram-ios-reference-menu.md. Uses the actual
// ported GlassBackgroundView/Container and Display layer spring implementation.
// No new selection, caption, permission or send authority lives in this view.
@MainActor
final class ZoenAttachmentReferenceMenu: UIView {
  struct Action {
    let id: String
    let title: String
    let image: UIImage?
    let enabled: Bool
    let perform: () -> Void
  }
  private let source: WeakSource
  private final class WeakSource {
    weak var view: UIView?
    init(_ view: UIView) { self.view = view }
  }
  private let actions: [Action]
  private let appearance: ZoenKeyboardPanelAppearance
  private let isValid: () -> Bool
  private let didDismiss: (ZoenAttachmentReferenceMenu) -> Void
  private let shield = UIControl()
  private let scroll = UIScrollView()
  private let glassContainer = ZoenTelegramAttachmentGlassContainerView(spacing: 28)
  private let surface = ZoenTelegramAttachmentGlassView()
  private let highlight = UIView()
  private var rows: [ReferenceActionRow] = []
  private var presentation: ZoenAttachmentPresentationOwner?
  private weak var admittedWindow: UIWindow?
  private var lifecycle = ZoenAttachmentReferenceMenuLifecycle()
  private var geometry: ZoenAttachmentReferenceMenuLayout.Geometry?
  private var highlightedIndex: Int?
  private let haptic = UISelectionFeedbackGenerator()
  private var observers: [NSObjectProtocol] = []
  private let reducedMotion: @MainActor () -> Bool
  private lazy var drag = ReferenceMenuSelectionRecognizer(target: self, action: #selector(dragged(_:)))

  init(source: UIView, actions: [Action], dismissLabel: String,
       appearance: ZoenKeyboardPanelAppearance, isValid: @escaping () -> Bool,
       reducedMotion: @escaping @MainActor () -> Bool = { UIAccessibility.isReduceMotionEnabled },
       didDismiss: @escaping (ZoenAttachmentReferenceMenu) -> Void) {
    self.source = WeakSource(source); self.actions = actions
    self.appearance = appearance; self.isValid = isValid
    self.didDismiss = didDismiss; self.reducedMotion = reducedMotion
    super.init(frame: .zero)
    accessibilityIdentifier = "attachment-reference-menu"
    accessibilityViewIsModal = true
    overrideUserInterfaceStyle = appearance.isDark ? .dark : .light
    backgroundColor = .clear
    shield.accessibilityLabel = dismissLabel
    shield.accessibilityTraits = .button; shield.isAccessibilityElement = true
    shield.accessibilityIdentifier = "attachment-reference-menu-dismiss"
    shield.addAction(UIAction { [weak self] _ in self?.dismiss(animated: true) }, for: .touchUpInside)
    // Keep a full-window clear cancellation surface; only menu contents scroll.
    addSubview(shield); addSubview(scroll)
    scroll.backgroundColor = .clear; scroll.contentInsetAdjustmentBehavior = .never
    scroll.showsVerticalScrollIndicator = false; scroll.alwaysBounceVertical = false
    scroll.delaysContentTouches = false; scroll.scrollsToTop = false
    scroll.addSubview(glassContainer)
    glassContainer.contentView.addSubview(surface)
    surface.contentView.clipsToBounds = true
    surface.contentView.addSubview(highlight)
    highlight.backgroundColor = appearance.isDark ? .white : .black
    highlight.isUserInteractionEnabled = false; highlight.alpha = 0
    for (index, action) in actions.enumerated() {
      let row = ReferenceActionRow(action: action, appearance: appearance)
      row.addAction(UIAction { [weak self] _ in self?.choose(index) }, for: .touchUpInside)
      surface.contentView.addSubview(row); rows.append(row)
    }
    drag.shouldBegin = { [weak self] point in
      guard let self, self.lifecycle.isInteractive, !self.scroll.isScrollEnabled else { return false }
      return !self.rows.indices.contains { self.rows[$0].frame.contains(point) && !self.actions[$0].enabled }
    }
    surface.addGestureRecognizer(drag)
    glassContainer.update(isDark: appearance.isDark)
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }

  @discardableResult func present(in window: UIWindow) -> Bool {
    guard lifecycle.phase == .idle, source.view?.window === window, isValid(),
          !actions.isEmpty, actions.count <= 10, actions.allSatisfy({ !$0.id.isEmpty && !$0.title.isEmpty }),
          let owner = ZoenAttachmentPresentationOwner(content: self, in: window,
            onWindowLoss: { [weak self] in self?.remove() }) else { return false }
    presentation = owner; admittedWindow = window
    guard lifecycle.present() else { owner.remove(); return false }
    setNeedsLayout(); layoutIfNeeded()
    guard lifecycle.isInteractive, geometry != nil else { remove(); return false }
    observers = [UIApplication.willResignActiveNotification, UIAccessibility.reduceMotionStatusDidChangeNotification].map { name in
      NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
        MainActor.assumeIsolated { self?.dismiss(animated: false) }
      }
    }
    UIImpactFeedbackGenerator(style: .medium).impactOccurred()
    haptic.prepare()
    if !reducedMotion(), let geometry {
      glassContainer.layer.animateAlpha(from: 0, to: 1, duration: ZoenAttachmentReferenceMenuLayout.openingOpacityDuration)
      glassContainer.layer.animateSpring(from: ZoenAttachmentReferenceMenuLayout.closedScale as NSNumber, to: 1 as NSNumber,
        keyPath: "transform.scale", duration: ZoenAttachmentReferenceMenuLayout.openingDuration,
        initialVelocity: 0, damping: ZoenAttachmentReferenceMenuLayout.springDamping)
      glassContainer.layer.animateSpring(from: NSValue(cgPoint: geometry.originOffset), to: NSValue(cgPoint: .zero),
        keyPath: "position", duration: ZoenAttachmentReferenceMenuLayout.openingDuration,
        initialVelocity: 0, damping: ZoenAttachmentReferenceMenuLayout.springDamping, additive: true, key: "reference-origin")
    } else {
      glassContainer.layer.animateAlpha(from: 0, to: 1, duration: ZoenAttachmentReferenceMenuLayout.openingOpacityDuration)
    }
    UIAccessibility.post(notification: .screenChanged, argument: rows.first)
    return true
  }

  /// Called after the real model/caption/visibility update. Existing closures
  /// still revalidate on activation; a stale open menu cannot regain authority.
  func invalidateIfNeeded() {
    guard lifecycle.isInteractive else { return }
    guard let window = admittedWindow, presentation?.isAttached(in: window) == true,
          source.view?.window === window, isValid(), sourceIsVisible else {
      dismiss(animated: false); return
    }
    setNeedsLayout()
  }
  private var sourceIsVisible: Bool {
    var view = source.view
    while let current = view {
      if current.isHidden || current.alpha < 0.01 { return false }
      view = current.superview
    }
    return true
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    guard lifecycle.isInteractive, let source = source.view, let window = admittedWindow,
          source.window === window, presentation?.isAttached(in: window) == true,
          sourceIsVisible, isValid(), bounds.width > 0 else {
      if lifecycle.isInteractive { dismiss(animated: false) }; return
    }
    shield.frame = bounds
    let font = UIFont.systemFont(ofSize: 17)
    let measured = rows.map { $0.measure(font: font, constrainedWidth: min(240, max(0, bounds.width - 32))) }
    let width = ZoenAttachmentReferenceMenuLayout.width(textWidths: measured.map(\.width),
      iconWidths: actions.map { $0.image?.size.width ?? 0 }, available: bounds.width)
    var y = ZoenAttachmentReferenceMenuLayout.listInset
    for (index, row) in rows.enumerated() {
      let height = measured[index].height + ZoenAttachmentReferenceMenuLayout.verticalInset * 2
      row.frame = CGRect(x: 0, y: y, width: width, height: height); y += height
    }
    y += ZoenAttachmentReferenceMenuLayout.listInset
    let next = ZoenAttachmentReferenceMenuLayout.geometry(viewport: bounds.size,
      source: source.convert(source.bounds, to: self), measuredSize: CGSize(width: width, height: y),
      intrinsicBottom: window.safeAreaInsets.bottom)
    let oldGeometry = geometry; geometry = next
    scroll.frame = bounds; scroll.contentSize = next.contentSize
    // A clear UIScrollView must not steal outside cancellation touches.
    scroll.isScrollEnabled = next.contentSize.height > bounds.height
    if oldGeometry == nil || oldGeometry?.menu != next.menu || oldGeometry?.contentSize != next.contentSize {
      scroll.contentOffset = CGPoint(x: 0, y: next.initialScrollY)
    }
    glassContainer.frame = next.menu; surface.frame = glassContainer.bounds
    surface.update(isDark: appearance.isDark, cornerRadius: min(30, y / 2), isInteractive: false)
    surface.contentView.layer.cornerRadius = min(30, y / 2)
    drag.isEnabled = !scroll.isScrollEnabled
    updateHighlight(animated: false)
  }
  override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
    guard !isHidden, alpha >= 0.01, isUserInteractionEnabled, bounds.contains(point) else { return nil }
    let menuPoint = convert(point, to: surface)
    if surface.bounds.contains(menuPoint) { return surface.hitTest(menuPoint, with: event) ?? surface }
    return shield
  }
  @objc private func dragged(_ recognizer: UIGestureRecognizer) {
    switch recognizer.state {
    case .began, .changed:
      highlight(at: recognizer.location(in: surface))
    case .ended:
      highlight(at: recognizer.location(in: surface))
      let index = highlightedIndex; setHighlighted(nil, animated: true, feedback: false)
      if let index { choose(index) }
    case .cancelled, .failed: setHighlighted(nil, animated: true, feedback: false)
    default: break
    }
  }
  func highlight(at point: CGPoint?) {
    guard lifecycle.isInteractive, isValid() else { return }
    let index = point.flatMap { point in rows.indices.first { rows[$0].frame.contains(point) && actions[$0].enabled } }
    setHighlighted(index, animated: true)
  }
  private func setHighlighted(_ index: Int?, animated: Bool, feedback: Bool = true) {
    guard highlightedIndex != index else { return }
    highlightedIndex = index
    if feedback { haptic.selectionChanged() }
    updateHighlight(animated: animated)
  }
  private func updateHighlight(animated: Bool) {
    let initialHighlight = highlightedIndex != nil && highlight.alpha == 0
    if initialHighlight, let index = highlightedIndex {
      let row = rows[index].frame
      highlight.frame = CGRect(x: 10, y: row.minY, width: max(0, row.width - 20), height: row.height)
      highlight.layer.cornerRadius = min(20, row.height / 2)
    }
    let duration: Double = highlightedIndex == nil || initialHighlight ? 0.2 : 0.16
    let apply = {
      if let index = self.highlightedIndex {
        let row = self.rows[index].frame
        self.highlight.frame = CGRect(x: 10, y: row.minY, width: max(0, row.width - 20), height: row.height)
        self.highlight.layer.cornerRadius = min(20, row.height / 2)
        self.highlight.alpha = 0.1
      } else { self.highlight.alpha = 0 }
    }
    if animated && !reducedMotion() {
      UIView.animate(withDuration: duration, delay: 0, options: [.beginFromCurrentState, .allowUserInteraction, .curveEaseInOut], animations: apply)
    } else { UIView.performWithoutAnimation(apply) }
  }
  func choose(_ index: Int) {
    guard lifecycle.isInteractive, actions.indices.contains(index), actions[index].enabled,
          let window = admittedWindow, presentation?.isAttached(in: window) == true,
          source.view?.window === window, sourceIsVisible, isValid() else { invalidateIfNeeded(); return }
    let perform = actions[index].perform
    // MediaPickerScreen:3174–3181 starts dismissal before the terminal action;
    // it does not postpone sending until an animation callback.
    dismiss(animated: true)
    perform()
  }
  func dismiss(animated: Bool) {
    guard lifecycle.dismiss() else { return }
    isUserInteractionEnabled = false; setHighlighted(nil, animated: false, feedback: false)
    guard animated, window != nil else { remove(); return }
    let layer = glassContainer.layer
    let visible = layer.presentation()
    let alpha = visible?.opacity ?? layer.opacity
    let scale = (visible?.value(forKeyPath: "transform.scale") as? NSNumber)?.doubleValue ?? 1
    let offset = visible.map { CGPoint(x: $0.position.x - layer.position.x, y: $0.position.y - layer.position.y) } ?? .zero
    var targetOffset = geometry?.originOffset ?? .zero
    if let source = source.view, source.window === admittedWindow {
      var sourceRect = source.convert(source.bounds, to: self).insetBy(dx: -2, dy: 0)
      sourceRect.size.width += 5
      let center = glassContainer.convert(CGPoint(x: glassContainer.bounds.midX, y: glassContainer.bounds.midY), to: self)
      targetOffset = CGPoint(x: sourceRect.midX - center.x, y: sourceRect.maxY - center.y)
    }
    layer.removeAllAnimations()
    layer.animateAlpha(from: CGFloat(alpha), to: 0, duration: ZoenAttachmentReferenceMenuLayout.closingDuration, removeOnCompletion: false)
    let finish: (Bool) -> Void = { [weak self] _ in self?.remove() }
    if reducedMotion() {
      layer.animateAlpha(from: CGFloat(alpha), to: 0, duration: ZoenAttachmentReferenceMenuLayout.closingDuration,
        removeOnCompletion: false, completion: finish)
    } else {
      layer.animate(from: scale as NSNumber, to: ZoenAttachmentReferenceMenuLayout.closedScale as NSNumber,
        keyPath: "transform.scale", timingFunction: CAMediaTimingFunctionName.easeInEaseOut.rawValue,
        duration: ZoenAttachmentReferenceMenuLayout.closingDuration, removeOnCompletion: false, completion: finish)
      layer.animate(from: NSValue(cgPoint: offset), to: NSValue(cgPoint: targetOffset), keyPath: "position",
        timingFunction: CAMediaTimingFunctionName.easeInEaseOut.rawValue, duration: ZoenAttachmentReferenceMenuLayout.closingDuration,
        removeOnCompletion: false, additive: true)
    }
  }
  /// Retirement interrupts an in-flight opening/closing and restores ownership
  /// synchronously. A superseded CA completion cannot execute any action.
  func retire() { remove() }
  private func remove() {
    guard lifecycle.remove() else { return }
    isUserInteractionEnabled = false
    observers.forEach(NotificationCenter.default.removeObserver); observers.removeAll()
    glassContainer.layer.removeAllAnimations(); highlight.layer.removeAllAnimations()
    presentation?.remove(); presentation = nil
    didDismiss(self)
    if let source = source.view, source.window === admittedWindow, sourceIsVisible, isValid() {
      UIAccessibility.post(notification: .layoutChanged, argument: source)
    }
  }
  override func accessibilityPerformEscape() -> Bool { dismiss(animated: true); return true }
}

@MainActor private final class ReferenceActionRow: UIControl {
  private let title = UILabel()
  private let icon = UIImageView()
  init(action: ZoenAttachmentReferenceMenu.Action, appearance: ZoenKeyboardPanelAppearance) {
    super.init(frame: .zero)
    title.text = action.title; title.numberOfLines = 2
    title.lineBreakMode = .byTruncatingTail
    title.textColor = action.enabled ? appearance.text : appearance.secondary
    icon.image = action.image; icon.tintColor = title.textColor
    icon.contentMode = .center
    title.isUserInteractionEnabled = false; icon.isUserInteractionEnabled = false
    addSubview(title); addSubview(icon)
    isEnabled = action.enabled; isExclusiveTouch = true
    isAccessibilityElement = true; accessibilityLabel = action.title
    accessibilityTraits = action.enabled ? .button : [.button, .notEnabled]
    accessibilityIdentifier = "attachment-reference-action-\(action.id)"
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
  func measure(font: UIFont, constrainedWidth: CGFloat) -> CGSize {
    title.font = font
    let right = icon.image == nil ? 18 : max(32, icon.image?.size.width ?? 0) + 8
    return title.sizeThatFits(CGSize(width: max(1, constrainedWidth - 18 - right), height: .greatestFiniteMagnitude))
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    let iconSize = icon.image?.size ?? .zero
    title.frame = CGRect(x: 18, y: 11, width: max(0, bounds.width - 18 - (icon.image == nil ? 18 : max(32, iconSize.width) + 20 + 8)), height: max(0, bounds.height - 22))
    icon.frame = CGRect(x: bounds.width - 20 - 32 + floor((32 - iconSize.width) / 2),
      y: floor((bounds.height - iconSize.height) / 2), width: iconSize.width, height: iconSize.height)
  }
}

/// ContextControllerActionsStackNode.ItemSelectionRecognizer:1405–1467.
/// Begin immediately on touch, instead of a pan recognizer's distance threshold.
@MainActor private final class ReferenceMenuSelectionRecognizer: UIGestureRecognizer {
  var shouldBegin: ((CGPoint) -> Bool)?
  override init(target: Any?, action: Selector?) {
    super.init(target: target, action: action)
    delaysTouchesBegan = false; delaysTouchesEnded = false
  }
  override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesBegan(touches, with: event)
    guard let location = touches.first?.location(in: view), shouldBegin?(location) != false else {
      state = .failed; return
    }
    state = .began
  }
  override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesMoved(touches, with: event); state = .changed
  }
  override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesEnded(touches, with: event); state = .ended
  }
  override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesCancelled(touches, with: event); state = .cancelled
  }
}
