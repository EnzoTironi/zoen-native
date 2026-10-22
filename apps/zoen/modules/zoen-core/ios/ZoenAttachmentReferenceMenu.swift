import UIKit

// UIKit translation of the modern reference/extracted ContextController at
// Telegram-iOS f1dd7a2 (GPL-2.0-or-later, GPLv3 selected). Exact source ranges
// and adaptations: docs/product/telegram-ios-{reference,file-extracted}-menu.md.
// Uses the actual
// ported GlassBackgroundView/Container and Display layer spring implementation.
// No new selection, caption, permission or send authority lives in this view.
@MainActor
class ZoenAttachmentContextMenu: UIView {
  // ContextSourceContainer.swift:17–25,410–423. Transparent rendered contents
  // keep the clear outside-dismiss surface in iOS 27 touch delivery; hitTest
  // alone is insufficient. Shared per class; no visible backdrop is introduced.
  private static let transparentHitTestImage: UIImage = {
    UIGraphicsBeginImageContextWithOptions(CGSize(width: 1, height: 1), false, 1)
    let image = UIGraphicsGetImageFromCurrentImageContext()
    UIGraphicsEndImageContext()
    return image ?? UIImage()
  }()
  struct Action {
    enum Invocation { case afterDismissalBegins, afterContentReturns }
    let id: String
    let title: String
    let image: UIImage?
    let enabled: Bool
    let invocation: Invocation
    let perform: () -> Void
    init(id: String, title: String, image: UIImage?, enabled: Bool,
         invocation: Invocation = .afterDismissalBegins, perform: @escaping () -> Void) {
      self.id = id; self.title = title; self.image = image; self.enabled = enabled
      self.invocation = invocation; self.perform = perform
    }
  }
  let source: WeakSource
  final class WeakSource {
    weak var view: UIView?
    init(_ view: UIView) { self.view = view }
  }
  let actions: [Action]
  let appearance: ZoenKeyboardPanelAppearance
  let isValid: () -> Bool
  let didDismiss: (ZoenAttachmentContextMenu) -> Void
  let extraction: ZoenAttachmentExtractedContent?
  let extractedStage = UIView()
  private var extractedGeometry: ZoenAttachmentExtractedMenuLayout.Geometry?
  var backdrop: ZoenAttachmentContextBackdrop?
  private let shield = UIControl()
  let scroll = UIScrollView()
  let glassContainer = ZoenTelegramAttachmentGlassContainerView(spacing: 28)
  let surface = ZoenTelegramAttachmentGlassView()
  let highlight = UIView()
  var rows: [ReferenceActionRow] = []
  var presentation: ZoenAttachmentPresentationOwner?
  weak var admittedWindow: UIWindow?
  var lifecycle = ZoenAttachmentReferenceMenuLifecycle()
  var geometry: ZoenAttachmentReferenceMenuLayout.Geometry?
  var highlightedIndex: Int?
  var externalInitialPoint: CGPoint?
  var externalMoved = false
  var returnedAction: (() -> Void)?
  var isReturningAfterAction = false
  let haptic = UISelectionFeedbackGenerator()
  var observers: [NSObjectProtocol] = []
  let reducedMotion: @MainActor () -> Bool
  private lazy var drag = ReferenceMenuSelectionRecognizer(target: self, action: #selector(dragged(_:)))

  init(source: UIView, extraction: ZoenAttachmentExtractedContent?, actions: [Action], dismissLabel: String,
       appearance: ZoenKeyboardPanelAppearance, isValid: @escaping () -> Bool,
       reducedMotion: @escaping @MainActor () -> Bool = { UIAccessibility.isReduceMotionEnabled },
       didDismiss: @escaping (ZoenAttachmentContextMenu) -> Void) {
    self.source = WeakSource(source); self.actions = actions
    self.extraction = extraction
    self.appearance = appearance; self.isValid = isValid
    self.didDismiss = didDismiss; self.reducedMotion = reducedMotion
    super.init(frame: .zero)
    accessibilityIdentifier = extraction == nil ? "attachment-reference-menu" : "attachment-extracted-menu"
    accessibilityViewIsModal = true
    overrideUserInterfaceStyle = appearance.isDark ? .dark : .light
    backgroundColor = .clear
    shield.accessibilityLabel = dismissLabel
    shield.accessibilityTraits = .button; shield.isAccessibilityElement = true
    shield.accessibilityIdentifier = extraction == nil ? "attachment-reference-menu-dismiss" : "attachment-extracted-menu-dismiss"
    shield.addAction(UIAction { [weak self] _ in self?.dismiss(animated: true) }, for: .touchUpInside)
    shield.layer.contents = Self.transparentHitTestImage.cgImage
    // Keep a full-window clear cancellation surface; only menu contents scroll.
    if extraction != nil {
      let backdrop = ZoenAttachmentContextBackdrop(isDark: appearance.isDark)
      self.backdrop = backdrop; addSubview(backdrop)
    }
    addSubview(shield); addSubview(scroll)
    scroll.backgroundColor = .clear; scroll.contentInsetAdjustmentBehavior = .never
    scroll.showsVerticalScrollIndicator = false; scroll.alwaysBounceVertical = false
    scroll.delaysContentTouches = false; scroll.scrollsToTop = false
    scroll.addSubview(glassContainer)
    if extraction != nil { scroll.addSubview(extractedStage); extractedStage.isUserInteractionEnabled = false }
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
            policy: extraction == nil ? .referenceContext : .extractedContext,
            onWindowLoss: { [weak self] in self?.remove() }) else { return false }
    presentation = owner; admittedWindow = window
    guard lifecycle.present() else { owner.remove(); return false }
    setNeedsLayout(); layoutIfNeeded()
    guard lifecycle.isInteractive, geometry != nil else { remove(); return false }
    if let extraction, !extraction.take(into: extractedStage) { remove(); return false }
    observers = [UIApplication.willResignActiveNotification, UIAccessibility.reduceMotionStatusDidChangeNotification].map { name in
      NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
        MainActor.assumeIsolated { self?.dismiss(animated: false) }
      }
    }
    UIImpactFeedbackGenerator(style: .medium).impactOccurred()
    haptic.prepare()
    backdrop?.layer.animateAlpha(from: 0, to: 1, duration: 0.2)
    if !reducedMotion(), let extractedGeometry {
      extractedStage.layer.animateSpring(from: extractedGeometry.contentOpeningOffset as NSNumber, to: 0 as NSNumber,
        keyPath: "position.y", duration: ZoenAttachmentReferenceMenuLayout.openingDuration,
        initialVelocity: 0, damping: ZoenAttachmentReferenceMenuLayout.springDamping, additive: true, key: "extracted-content-origin")
    }
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
  var sourceIsVisible: Bool {
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
    shield.frame = bounds; backdrop?.frame = bounds
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
    let sourceFrame = source.convert(source.bounds, to: self)
    // UIKit owns the table's rotation/reuse layout. Return its real content
    // synchronously before a changed row size could stretch an extracted body.
    if extraction != nil, let previous = geometry, previous.source.size != sourceFrame.size {
      dismiss(animated: false); return
    }
    let next: ZoenAttachmentReferenceMenuLayout.Geometry
    if let extraction {
      let extracted = ZoenAttachmentExtractedMenuLayout.geometry(viewport: bounds.size, source: sourceFrame,
        menuSize: CGSize(width: width, height: y), statusBarHeight: window.windowScene?.statusBarManager?.statusBarFrame.height ?? 0,
        intrinsicBottom: window.safeAreaInsets.bottom)
      extractedGeometry = extracted
      next = .init(source: extracted.content, menu: extracted.menu, contentSize: extracted.contentSize,
        initialScrollY: extracted.initialScrollY, originOffset: extracted.menuOpeningOffset)
      extraction.layout(in: extractedStage, frame: extracted.content)
    } else {
      next = ZoenAttachmentReferenceMenuLayout.geometry(viewport: bounds.size,
        source: sourceFrame, measuredSize: CGSize(width: width, height: y), intrinsicBottom: window.safeAreaInsets.bottom)
    }
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

  override func accessibilityPerformEscape() -> Bool { dismiss(animated: true); return true }
}

/// Photos uses the plain reference contract: no extraction, dim or blur.
@MainActor final class ZoenAttachmentReferenceMenu: ZoenAttachmentContextMenu {
  init(source: UIView, actions: [Action], dismissLabel: String,
       appearance: ZoenKeyboardPanelAppearance, isValid: @escaping () -> Bool,
       reducedMotion: @escaping @MainActor () -> Bool = { UIAccessibility.isReduceMotionEnabled },
       didDismiss: @escaping (ZoenAttachmentReferenceMenu) -> Void) {
    super.init(source: source, extraction: nil, actions: actions, dismissLabel: dismissLabel,
      appearance: appearance, isValid: isValid, reducedMotion: reducedMotion,
      didDismiss: { menu in if let menu = menu as? ZoenAttachmentReferenceMenu { didDismiss(menu) } })
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
}

/// File uses the distinct actual-content take/put-back contract.
@MainActor final class ZoenAttachmentExtractedMenu: ZoenAttachmentContextMenu {
  init(source: ZoenAttachmentExtractedContent, actions: [Action], dismissLabel: String,
       appearance: ZoenKeyboardPanelAppearance, isValid: @escaping () -> Bool,
       reducedMotion: @escaping @MainActor () -> Bool = { UIAccessibility.isReduceMotionEnabled },
       didDismiss: @escaping (ZoenAttachmentExtractedMenu) -> Void) {
    super.init(source: source.anchor, extraction: source, actions: actions, dismissLabel: dismissLabel,
      appearance: appearance, isValid: isValid, reducedMotion: reducedMotion,
      didDismiss: { menu in if let menu = menu as? ZoenAttachmentExtractedMenu { didDismiss(menu) } })
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
}
