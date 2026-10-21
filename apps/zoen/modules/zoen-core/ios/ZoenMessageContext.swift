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
    if nearestStage?.contextGesture.source === self { nearestStage?.contextGesture.cancelAdmission() }
    presentation?.retire()
    resetPress(animated: false)
  }
  var gestureAdmitted: Bool {
    active && presentation == nil && children.count == 1 && window != nil &&
      window?.windowScene?.activationState == .foregroundActive && nearestStage?.active == true && nearestStage?.controller == nil
  }
  func dispatch(kind: String) {
    guard gestureAdmitted else { return }
    let gesture = UUID()
    lastGesture = gesture
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
      request: request, generation: generation, binding: binding, child: children[0])
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

struct ZoenMessageContextConfiguration: Decodable {
  struct Action: Decodable {
    let id: String
    let label: String
    var symbol: String? {
      switch id {
      case "reply": "arrowshape.turn.up.left"
      case "thread": "bubble.left.and.bubble.right"
      case "copy": "doc.on.doc"
      case "edit": "pencil"
      case "recover": "arrow.clockwise"
      case "forward": "arrowshape.turn.up.right"
      case "share": "square.and.arrow.up"
      case "delete": "trash"
      default: nil
      }
    }
  }
  struct Reaction: Decodable { let emoji: String; let label: String; let selected: Bool }
  struct Theme: Decodable {
    let background: String, highlight: String, text: String, secondary: String
    let separator: String, destructive: String, dim: String
    let isDark: Bool
  }
  let actions: [Action]
  let reactions: [Reaction]
  let moreLabel: String?
  let theme: Theme
  let reducedMotion: Bool
  let incoming: Bool
  static let emoji = Set(["👍", "❤️", "😂", "😮", "😢", "🙏"])
  static func decode(_ json: String) throws -> Self {
    guard let data = json.data(using: .utf8), data.count <= 16_384 else { throw ZoenMessageContextSource.Failure.configuration }
    let config = try JSONDecoder().decode(Self.self, from: data)
    guard config.actions.count <= 7, config.reactions.count <= 6,
          !config.actions.isEmpty || !config.reactions.isEmpty,
          Set(config.actions.map(\.id)).count == config.actions.count,
          Set(config.reactions.map(\.emoji)).count == config.reactions.count,
          config.actions.allSatisfy({ $0.symbol != nil && label($0.label) }),
          config.reactions.allSatisfy({ emoji.contains($0.emoji) && label($0.label) }),
          config.moreLabel.map(label) ?? true,
          [config.theme.background, config.theme.highlight, config.theme.text, config.theme.secondary,
           config.theme.separator, config.theme.destructive, config.theme.dim].allSatisfy({ UIColor.contextHex($0) != nil }) else {
      throw ZoenMessageContextSource.Failure.configuration
    }
    return config
  }
  private static func label(_ value: String) -> Bool {
    !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && value.utf8.count <= 256
  }
}

@MainActor
final class ZoenMessageContextController: UIViewController {
  let request: UUID
  let binding: String
  let sourceGeneration: UInt64
  weak var child: UIView?
  weak var sourceWindow: UIWindow?
  weak var source: ZoenMessageContextSource?
  weak var stage: ZoenMessageContextStage?
  var complete: ((Result<String?, Error>) -> Void)?
  private let configuration: ZoenMessageContextConfiguration
  private let backdrop = UIView()
  private let material = UIVisualEffectView()
  private let actions = UIScrollView()
  private let reactionScroll = UIScrollView()
  private let preview = UIScrollView()
  private var actionButtons: [UIButton] = []
  private var reactionButtons: [UIButton] = []
  private var selected: String?
  private var closing = false
  private var retired = false
  private var finished = false
  private var extracted = false
  private var waiters: [CheckedContinuation<Void, Never>] = []
  private var returnAnimator: UIViewPropertyAnimator?
  private var sourceFrame = CGRect.zero
  private var pressedScale: CGFloat = 1
  private var hover: UIButton?
  private var heldSelectionAdmitted = false
  private var reduceMotion: Bool { configuration.reducedMotion || UIAccessibility.isReduceMotionEnabled }

  init(source: ZoenMessageContextSource, configuration: ZoenMessageContextConfiguration,
       request: UUID, generation: UInt64, binding: String, child: UIView) {
    self.source = source; self.configuration = configuration; self.request = request
    sourceGeneration = generation; self.binding = binding; self.child = child
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
    button.addAction(UIAction(identifier: UIAction.Identifier(id)) { [weak self] _ in self?.choose(id) }, for: .touchUpInside)
    button.addAction(UIAction { [weak self, weak button] _ in
      guard let self, let button else { return }
      button.backgroundColor = UIColor.contextHex(self.configuration.theme.highlight)
    }, for: .touchDown)
    button.addAction(UIAction { [weak self, weak button] _ in
      guard let self, let button else { return }
      button.backgroundColor = button.isSelected ? UIColor.contextHex(self.configuration.theme.highlight) : .clear
    }, for: [.touchUpInside, .touchCancel, .touchUpOutside])
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
    let menuWidth = min(240, safe.width)
    let rowHeights = actionButtons.map { button in
      max(44, ceil(button.titleLabel?.sizeThatFits(CGSize(width: menuWidth - 78, height: .greatestFiniteMagnitude)).height ?? 22) + 22)
    }
    let totalHeight = rowHeights.reduce(0, +)
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
    var actionY: CGFloat = 0
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
  func trackHeldTouch(_ point: CGPoint, origin: CGPoint, ended: Bool, cancelled: Bool) {
    guard isCurrent, !closing else { return }
    if cancelled || ended {
      // Reflow can put a menu row under a stationary finger. Only a preceding
      // deliberate Move may admit its hover; Up must never hit-test a new row.
      let selected = !cancelled && heldSelectionAdmitted ? hover : nil
      heldSelectionAdmitted = false
      updateHeldHover(nil)
      if let id = selected?.accessibilityIdentifier?.replacingOccurrences(of: "message-context-", with: "") { choose(id) }
      return
    }
    // Telegram f1dd7a2 ContextControllerImpl ContextGesture branch: vertical >4pt.
    // The origin is the original held UITouch in the same window coordinates.
    if !heldSelectionAdmitted && abs(point.y - origin.y) > 4 { heldSelectionAdmitted = true }
    guard heldSelectionAdmitted else { return }
    let target = (reactionButtons + actionButtons).first { button in
      guard let parent = button.superview, !parent.isHidden else { return false }
      let local = button.convert(point, from: sourceWindow)
      return button.bounds.contains(local) && parent.bounds.contains(parent.convert(point, from: sourceWindow))
    }
    updateHeldHover(target)
  }
  private func updateHeldHover(_ target: UIButton?) {
    if hover !== target {
      hover?.transform = .identity
      hover?.backgroundColor = hover?.isSelected == true ? UIColor.contextHex(configuration.theme.highlight) : .clear
      hover = target
      target?.backgroundColor = UIColor.contextHex(configuration.theme.highlight)
      if let target, !reduceMotion { target.transform = CGAffineTransform(scaleX: 1.15, y: 1.15); UISelectionFeedbackGenerator().selectionChanged() }
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
  @objc private func nextAction() { moveFocus(1) }
  @objc private func previousAction() { moveFocus(-1) }
  @objc private func activateFocused() {
    if let id = hover?.accessibilityIdentifier?.replacingOccurrences(of: "message-context-", with: "") { choose(id) }
  }
  private func moveFocus(_ direction: Int) {
    guard isCurrent, !closing else { return }
    let buttons = reactionButtons + actionButtons
    guard !buttons.isEmpty else { return }
    let index = hover.flatMap { current in buttons.firstIndex(where: { $0 === current }) } ?? (direction > 0 ? -1 : 0)
    hover?.backgroundColor = hover?.isSelected == true ? UIColor.contextHex(configuration.theme.highlight) : .clear
    let next = buttons[(index + direction + buttons.count) % buttons.count]
    hover = next
    next.backgroundColor = UIColor.contextHex(configuration.theme.highlight)
    if let scroll = next.superview as? UIScrollView { scroll.scrollRectToVisible(next.frame, animated: !reduceMotion) }
    UIAccessibility.post(notification: .layoutChanged, argument: next)
  }
  @objc private func outside() { choose(nil) }
  @objc private func background() { retire() }
  @objc private func motionChanged() {
    if reduceMotion {
      for layer in [preview.layer, actions.layer, reactionScroll.layer, backdrop.layer] { layer.removeAllAnimations() }
      if closing { finish(); return }
      reflow(animated: false)
    }
  }
  private func choose(_ value: String?) {
    guard isCurrent, !closing else { return }
    let allowed = Set(configuration.actions.map(\.id) + configuration.reactions.map { "reaction:\($0.emoji)" } + (configuration.moreLabel == nil ? [] : ["more"]))
    guard value == nil || allowed.contains(value!) else { return }
    selected = value
    beginClose(animated: true)
  }
  func close() async {
    if finished { return }
    await withCheckedContinuation { waiter in
      waiters.append(waiter)
      selected = nil
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
    stage?.remove(self)
    if viewIfLoaded?.superview != nil || parent != nil { return }
    finished = true
    source?.finished(self)
    returnAnimator = nil
    NotificationCenter.default.removeObserver(self)
    let completion = complete; complete = nil
    if let error { completion?(.failure(error)) } else { completion?(.success(action)) }
    let draining = waiters; waiters.removeAll()
    for waiter in draining { waiter.resume() }
    if !retired, let source, source.window != nil { UIAccessibility.post(notification: .layoutChanged, argument: source) }
  }
  private func animateEntry() {
    UIView.animate(withDuration: reduceMotion ? 0 : 0.2) { self.backdrop.alpha = 1 }
    actions.alpha = 1; reactionScroll.alpha = 1
    guard !reduceMotion else { return }
    if let source, pressedScale < 1 { spring(source.contentHost.layer, key: "transform.scale", from: pressedScale, to: 1) }
    spring(preview.layer, key: "position", from: NSValue(cgPoint: CGPoint(x: sourceFrame.midX, y: sourceFrame.midY)), to: NSValue(cgPoint: preview.center))
    for (bar, duration) in [(actions, 0.05), (reactionScroll, 0.15)] {
      let alpha = CABasicAnimation(keyPath: "opacity"); alpha.fromValue = 0; alpha.toValue = 1; alpha.duration = duration
      bar.layer.add(alpha, forKey: "context.opacity")
      spring(bar.layer, key: "transform.scale", from: 0.01, to: 1)
    }
    for button in reactionButtons {
      let center = button.convert(CGPoint(x: button.bounds.midX, y: button.bounds.midY), to: view)
      let distance = abs(center.x - sourceFrame.midX) / max(1, reactionScroll.bounds.width)
      let animation = CASpringAnimation(keyPath: "transform.scale")
      animation.mass = 5; animation.stiffness = 900; animation.damping = 104
      animation.fromValue = 0.01; animation.toValue = 1
      animation.duration = animation.settlingDuration; animation.speed = Float(animation.settlingDuration / 0.42)
      animation.beginTime = CACurrentMediaTime() + 0.01 + Double(distance) * 0.3
      animation.fillMode = .backwards
      button.layer.add(animation, forKey: "context.item")
    }
  }
  private func spring(_ layer: CALayer, key: String, from: Any, to: Any) {
    let animation = CASpringAnimation(keyPath: key)
    animation.mass = 5; animation.stiffness = 900; animation.damping = 104; animation.initialVelocity = 0
    animation.fromValue = from; animation.toValue = to
    animation.duration = animation.settlingDuration; animation.speed = Float(animation.settlingDuration / 0.42)
    layer.add(animation, forKey: "context.\(key)")
  }
}

private extension CGRect {
  var contextFinite: Bool { [minX, minY, width, height].allSatisfy(\.isFinite) && width > 0 && height > 0 }
}
private extension UIColor {
  static func contextHex(_ value: String) -> UIColor? {
    guard value.first == "#", [7, 9].contains(value.count), let number = UInt64(value.dropFirst(), radix: 16) else { return nil }
    let alpha = value.count == 9 ? CGFloat(number & 255) / 255 : 1
    let rgb = value.count == 9 ? number >> 8 : number
    return UIColor(red: CGFloat((rgb >> 16) & 255) / 255, green: CGFloat((rgb >> 8) & 255) / 255, blue: CGFloat(rgb & 255) / 255, alpha: alpha)
  }
}

@MainActor
private final class ZoenContextActionButton: UIButton {
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
