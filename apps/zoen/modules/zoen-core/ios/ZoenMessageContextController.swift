import ExpoModulesCore
import UIKit

@MainActor
final class ZoenMessageContextController: UIViewController {
  let request: UUID
  let binding: String
  let gestureId: String
  weak var heldText: UITextView?
  let heldTextPoint: CGPoint
  let hadHeldText: Bool
  var textSelectionInitialized = false
  var selectingText = false
  var selectionAnnounced = false
  var textSelectionState: [ZoenContextTextSelectionState] = []
  var textSelectionInteraction: [(UIView, Bool)] = []
  let sourceGeneration: UInt64
  weak var child: UIView?
  weak var sourceWindow: UIWindow?
  weak var source: ZoenMessageContextSource?
  weak var stage: ZoenMessageContextStage?
  var complete: ((Result<String?, Error>) -> Void)?
  let configuration: ZoenMessageContextConfiguration
  let backdrop = UIView()
  private let material = UIVisualEffectView()
  let actions = UIScrollView()
  let actionHighlight = UIView()
  let reactionScroll = UIScrollView()
  let preview = UIScrollView()
  var actionButtons: [UIButton] = []
  var reactionButtons: [UIButton] = []
  var selected: String?
  var closing = false
  private var retired = false
  var finished = false
  private var extracted = false
  private var waiters: [CheckedContinuation<Void, Never>] = []
  private var returnAnimator: UIViewPropertyAnimator?
  var sourceFrame = CGRect.zero
  var pressedScale: CGFloat = 1
  var hover: UIButton?
  var heldSelectionAdmitted = false
  var reduceMotion: Bool { configuration.reducedMotion || UIAccessibility.isReduceMotionEnabled }

  init(source: ZoenMessageContextSource, configuration: ZoenMessageContextConfiguration,
       request: UUID, generation: UInt64, binding: String, child: UIView, gestureId: String = "", heldText: UITextView? = nil, heldTextPoint: CGPoint = .zero, hadHeldText: Bool = false) {
    self.source = source; self.configuration = configuration; self.request = request
    self.heldText = heldText; self.heldTextPoint = heldTextPoint; self.hadHeldText = hadHeldText || heldText != nil
    sourceGeneration = generation; self.binding = binding; self.child = child; self.gestureId = gestureId
    super.init(nibName: nil, bundle: nil)
    NotificationCenter.default.addObserver(self, selector: #selector(background), name: UIApplication.willResignActiveNotification, object: nil)
    NotificationCenter.default.addObserver(self, selector: #selector(motionChanged), name: UIAccessibility.reduceMotionStatusDidChangeNotification, object: nil)
  }
  @available(*, unavailable)
  required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }
  var isCurrent: Bool { !retired && !finished && (source?.current(self) ?? false) && stage?.controller === self && (stage?.active ?? false) }
  func admit(stage: ZoenMessageContextStage) throws {
    guard let source, source.bounds.contextFinite, source.window === stage.window,
          source.isDescendant(of: stage.contentHost), stage.bounds.contextFinite else { throw ZoenMessageContextSource.Failure.unavailable }
    self.stage = stage
    sourceWindow = source.window
    sourceFrame = source.convert(source.bounds, to: stage.overlayHost)
  }
  override func loadView() {
    view = UIView()
    view.backgroundColor = .clear
    view.accessibilityViewIsModal = true
    backdrop.backgroundColor = UIColor.contextHex(configuration.theme.dim)
    backdrop.alpha = 0
    backdrop.addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(outside)))
    view.addSubview(backdrop)
    for scroll in [reactionScroll, actions] {
      scroll.backgroundColor = .clear
      scroll.showsHorizontalScrollIndicator = false
      scroll.showsVerticalScrollIndicator = false
      scroll.clipsToBounds = true
    }
    material.effect = UIBlurEffect(style: configuration.theme.isDark ? .systemMaterialDark : .systemMaterialLight)
    material.backgroundColor = UIColor.contextHex(configuration.theme.background)
    actions.addSubview(material)
    actionHighlight.isUserInteractionEnabled = false
    actionHighlight.alpha = 0
    actionHighlight.backgroundColor = configuration.theme.isDark ? .white : .black
    actions.addSubview(actionHighlight)
    for action in configuration.actions {
      let button = makeButton(label: action.label, id: action.id, actionRow: true)
      button.setImage(action.symbol.flatMap { UIImage(systemName: $0, withConfiguration: UIImage.SymbolConfiguration(pointSize: 20, weight: .regular)) }, for: .normal)
      button.tintColor = UIColor.contextHex(action.id == "delete" ? configuration.theme.destructive : configuration.theme.text)
      button.setTitleColor(button.tintColor, for: .normal)
      button.contentHorizontalAlignment = .leading
      button.titleLabel?.font = .preferredFont(forTextStyle: .body)
      button.titleLabel?.adjustsFontForContentSizeCategory = true
      button.titleLabel?.numberOfLines = 2
      actions.addSubview(button)
      actionButtons.append(button)
    }
    for reaction in configuration.reactions {
      let button = makeButton(label: reaction.emoji, id: "reaction:\(reaction.emoji)")
      button.accessibilityLabel = reaction.label
      button.isSelected = reaction.selected
      if reaction.selected { button.accessibilityTraits.insert(.selected) }
      button.titleLabel?.font = .systemFont(ofSize: 30)
      button.backgroundColor = reaction.selected ? UIColor.contextHex(configuration.theme.highlight) : .clear
      button.layer.cornerRadius = 22
      reactionScroll.addSubview(button)
      reactionButtons.append(button)
    }
    if let label = configuration.moreLabel {
      let button = makeButton(label: nil, id: "more")
      button.setImage(UIImage(systemName: "chevron.down", withConfiguration: UIImage.SymbolConfiguration(pointSize: 17, weight: .semibold)), for: .normal)
      button.tintColor = UIColor.contextHex(configuration.theme.secondary)
      button.accessibilityLabel = label
      reactionScroll.addSubview(button)
      reactionButtons.append(button)
    }
    reactionScroll.backgroundColor = UIColor.contextHex(configuration.theme.background)
    preview.clipsToBounds = true
    preview.showsVerticalScrollIndicator = true
    preview.bounces = false
    view.addSubview(preview)
    view.addSubview(actions)
    view.addSubview(reactionScroll)
    actions.alpha = 0; reactionScroll.alpha = 0
  }
  private func makeButton(label: String?, id: String, actionRow: Bool = false) -> UIButton {
    let button: UIButton = actionRow ? ZoenContextActionButton(frame: .zero) : UIButton(type: .custom)
    button.setTitle(label, for: .normal)
    button.accessibilityIdentifier = "message-context-\(id)"
    button.accessibilityTraits = .button
    if let row = button as? ZoenContextActionButton {
      row.trackTouch = { [weak self] point, ended, cancelled in
        self?.trackActionTouch(point, ended: ended, cancelled: cancelled)
      }
    }
    button.addAction(UIAction(identifier: UIAction.Identifier(id)) { [weak self] _ in self?.choose(id) }, for: .touchUpInside)
    button.addAction(UIAction { [weak self, weak button] _ in
      guard let self, let button else { return }
      self.updateSelection(button, animated: true)
    }, for: [.touchDown, .touchDragEnter])
    button.addAction(UIAction { [weak self, weak button] _ in
      guard let self, self.hover === button else { return }
      self.updateSelection(nil, animated: true)
    }, for: [.touchUpInside, .touchCancel, .touchUpOutside, .touchDragExit])
    return button
  }
  func extract() {
    guard isCurrent, !extracted, let source else { retire(); return }
    extracted = true
    let host = source.contentHost
    pressedScale = host.layer.presentation()?.transform.m11 ?? host.layer.transform.m11
    host.layer.removeAllAnimations()
    host.transform = .identity
    source.cancelSurfaceTouch()
    host.removeFromSuperview()
    preview.addSubview(host)
    host.frame = source.bounds
    view.layoutIfNeeded()
    reflow(animated: false)
    animateEntry()
    if !reduceMotion { UIImpactFeedbackGenerator(style: .medium).impactOccurred() }
    becomeFirstResponder()
    UIAccessibility.post(notification: .screenChanged, argument: reactionButtons.first ?? actionButtons.first)
  }
  override func viewDidLayoutSubviews() {
    super.viewDidLayoutSubviews()
    backdrop.frame = view.bounds
    if extracted { reflow() }
  }
  func reflow(animated: Bool = true) {
    guard extracted, !closing, isCurrent, let source, let window = sourceWindow,
          source.contentHost.superview === preview else { return }
    let safe = view.convert(window.bounds.inset(by: window.safeAreaInsets), from: window).intersection(view.bounds).insetBy(dx: 8, dy: 8)
    guard safe.contextFinite, source.bounds.contextFinite, source.bounds.width <= safe.width else { retire(); return }
    let original = source.convert(source.bounds, to: view)
    if selectingText {
      layoutTextSelection(safe: safe, original: original, source: source)
      return
    }
    let menuWidth = min(240, safe.width)
    let rowHeights = actionButtons.map { button in
      max(44, ceil(button.titleLabel?.sizeThatFits(CGSize(width: menuWidth - 78, height: .greatestFiniteMagnitude)).height ?? 22) + 22)
    }
    let totalHeight = rowHeights.reduce(0, +) + (actionButtons.isEmpty ? 0 : 20)
    let actionHeight = min(totalHeight, max(44, safe.height * 0.44))
    let reactionHeight: CGFloat = reactionButtons.isEmpty ? 0 : 63
    let lowerGap: CGFloat = actionButtons.isEmpty ? 0 : 8
    guard safe.height >= actionHeight + reactionHeight + lowerGap + 44 else { retire(); return }
    let previewHeight = min(source.bounds.height, max(44, safe.height - actionHeight - reactionHeight - lowerGap))
    let x = min(max(original.minX, safe.minX), safe.maxX - source.bounds.width)
    let y = min(max(original.minY, safe.minY + reactionHeight), safe.maxY - previewHeight - actionHeight - lowerGap)
    let next = CGRect(x: x, y: y, width: source.bounds.width, height: previewHeight)
    let previous = preview.layer.presentation()?.position ?? preview.layer.position
    preview.layer.removeAllAnimations()
    preview.frame = next
    preview.contentSize = source.bounds.size
    source.contentHost.frame = source.bounds
    if animated && !reduceMotion { spring(preview.layer, key: "position", from: NSValue(cgPoint: previous), to: NSValue(cgPoint: preview.center)) }
    let menuX = configuration.incoming ? original.minX : original.maxX - menuWidth
    actions.frame = CGRect(x: min(max(menuX, safe.minX), safe.maxX - menuWidth), y: next.maxY + lowerGap, width: menuWidth, height: actionButtons.isEmpty ? 0 : actionHeight)
    actions.layer.cornerRadius = min(30, actionHeight * 0.5)
    actions.contentSize = CGSize(width: menuWidth, height: totalHeight)
    material.frame = CGRect(x: 0, y: 0, width: menuWidth, height: max(actionHeight, totalHeight))
    var actionY: CGFloat = 10
    for (index, button) in actionButtons.enumerated() {
      let rowHeight = rowHeights[index]
      button.frame = CGRect(x: 0, y: actionY, width: menuWidth, height: rowHeight)
      actionY += rowHeight
      button.imageView?.contentMode = .scaleAspectFit
      // Source: iconSideInset20 + standardIconWidth40; text begins at60.
      if let button = button as? ZoenContextActionButton {
        button.separator.backgroundColor = UIColor.contextHex(configuration.theme.separator)
        button.separator.isHidden = index == actionButtons.count - 1
        button.pixel = 1 / window.screen.scale
      }
    }
    layoutActionHighlight()
    let reactionContentWidth = CGFloat(reactionButtons.count) * 36 + CGFloat(max(0, reactionButtons.count - 1)) * 8 + 12
    let reactionWidth = min(safe.width, reactionContentWidth)
    let reactionX = configuration.incoming ? next.maxX - reactionWidth + 22 : next.minX - 26
    reactionScroll.frame = CGRect(x: min(max(reactionX, safe.minX), safe.maxX - reactionWidth), y: next.minY - reactionHeight, width: reactionWidth, height: reactionHeight == 0 ? 0 : 56)
    reactionScroll.layer.cornerRadius = 28
    reactionScroll.contentSize = CGSize(width: reactionContentWidth, height: 56)
    for (index, button) in reactionButtons.enumerated() {
      button.frame = CGRect(x: 2 + CGFloat(index) * 44, y: 6, width: 44, height: 44)
    }
  }
  override var canBecomeFirstResponder: Bool { true }
  override var keyCommands: [UIKeyCommand]? {
    [UIKeyCommand(input: UIKeyCommand.inputEscape, modifierFlags: [], action: #selector(outside)),
     UIKeyCommand(input: UIKeyCommand.inputDownArrow, modifierFlags: [], action: #selector(nextAction)),
     UIKeyCommand(input: UIKeyCommand.inputUpArrow, modifierFlags: [], action: #selector(previousAction)),
     UIKeyCommand(input: "\r", modifierFlags: [], action: #selector(activateFocused))]
  }
  override func accessibilityPerformEscape() -> Bool { choose(nil); return true }
  @objc private func outside() { choose(nil) }
  @objc func background() { retire() }
  @objc private func motionChanged() {
    if reduceMotion {
      for layer in [preview.layer, actions.layer, actionHighlight.layer, reactionScroll.layer, backdrop.layer] { layer.removeAllAnimations() }
      for button in reactionButtons { button.transform = .identity }
      if closing { finish(); return }
      reflow(animated: false)
    }
  }
  func choose(_ value: String?) {
    guard isCurrent, !closing else { return }
    let allowed = Set(configuration.actions.map(\.id) + configuration.reactions.map { "reaction:\($0.emoji)" } + (configuration.moreLabel == nil ? [] : ["more"]))
    guard value == nil || (!selectingText && allowed.contains(value!)) else { return }
    if value == "select" { beginTextSelection(); return }
    selected = value
    beginClose(animated: true)
  }
  func close(selectionResult: String? = nil) async {
    if finished { return }
    await withCheckedContinuation { waiter in
      waiters.append(waiter)
      selected = selectionResult
      beginClose(animated: true)
    }
  }
  func retire() {
    guard !finished else { return }
    retired = true; selected = nil
    beginClose(animated: false)
  }
  func failed(_ error: Error) {
    guard !finished else { return }
    retired = true
    finish(error: error)
  }
  private func beginClose(animated: Bool) {
    if closing {
      if !animated { returnAnimator?.stopAnimation(true); returnAnimator = nil; finish() }
      return
    }
    closing = true
    view.isUserInteractionEnabled = false
    guard animated, !reduceMotion, extracted, isCurrent, let source else { finish(); return }
    let previous = preview.layer.presentation()?.position ?? preview.layer.position
    preview.layer.removeAllAnimations()
    preview.center = previous
    for bar in [actions, reactionScroll, backdrop] {
      let opacity = bar.layer.presentation()?.opacity ?? bar.layer.opacity
      let transform = bar.layer.presentation()?.affineTransform() ?? bar.transform
      bar.layer.removeAllAnimations()
      bar.alpha = CGFloat(opacity)
      bar.transform = transform
    }
    let host = source.contentHost
    let hostTransform = host.layer.presentation()?.affineTransform() ?? host.transform
    host.layer.removeAllAnimations()
    host.transform = hostTransform
    let target = source.convert(source.bounds, to: view)
    let animator = UIViewPropertyAnimator(duration: 0.2, curve: .easeInOut) {
      self.preview.frame = CGRect(origin: target.origin, size: self.preview.bounds.size)
      host.transform = .identity
      self.backdrop.alpha = 0; self.actions.alpha = 0; self.reactionScroll.alpha = 0
    }
    returnAnimator = animator
    animator.addCompletion { [weak self] _ in self?.finish() }
    animator.startAnimation()
  }
  func restoreSource() { source?.restore(self) }
  private func finish(error: Error? = nil) {
    guard !finished else { return }
    let action = isCurrent ? selected : nil
    // isCurrent must be sampled BEFORE removal invalidates stage.controller.
    restoreTextSelection()
    stage?.remove(self)
    if viewIfLoaded?.superview != nil || parent != nil { return }
    finished = true
    if selectionAnnounced {
      selectionAnnounced = false
      source?.selectionPhase(self, active: false)
    }
    source?.finished(self)
    returnAnimator = nil
    NotificationCenter.default.removeObserver(self)
    let completion = complete; complete = nil
    if let error { completion?(.failure(error)) } else { completion?(.success(action)) }
    let draining = waiters; waiters.removeAll()
    for waiter in draining { waiter.resume() }
    if !retired, let source, source.window != nil { UIAccessibility.post(notification: .layoutChanged, argument: source) }
  }
}

@MainActor
private final class ZoenContextActionButton: UIButton {
  var trackTouch: ((CGPoint, Bool, Bool) -> Void)?
  override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
    if let touch = touches.first { trackTouch?(touch.location(in: window), false, false) }
  }
  override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
    if let touch = touches.first { trackTouch?(touch.location(in: window), false, false) }
  }
  override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
    if let touch = touches.first { trackTouch?(touch.location(in: window), true, false) }
  }
  override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
    trackTouch?(.zero, false, true)
  }
  override func accessibilityActivate() -> Bool {
    sendActions(for: .touchUpInside)
    return true
  }
  let separator = UIView()
  var pixel: CGFloat = 1
  override init(frame: CGRect) {
    super.init(frame: frame)
    separator.isUserInteractionEnabled = false
    addSubview(separator)
  }
  @available(*, unavailable)
  required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }
  override func layoutSubviews() {
    super.layoutSubviews()
    if let imageView, let image = imageView.image {
      let size = image.size
      imageView.frame = CGRect(x: 20 + floor((32 - size.width) * 0.5), y: floor((bounds.height - size.height) * 0.5), width: size.width, height: size.height)
    }
    if let titleLabel {
      let size = titleLabel.sizeThatFits(CGSize(width: max(1, bounds.width - 78), height: .greatestFiniteMagnitude))
      titleLabel.frame = CGRect(x: 60, y: floor((bounds.height - size.height) * 0.5), width: max(1, bounds.width - 78), height: size.height)
    }
    separator.frame = CGRect(x: 0, y: bounds.height - pixel, width: bounds.width, height: pixel)
  }
}
