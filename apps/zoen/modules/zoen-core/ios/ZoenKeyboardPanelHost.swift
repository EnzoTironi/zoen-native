import ExpoModulesCore
import UIKit

@MainActor
struct ZoenKeyboardPanelAppearance {
  let background: UIColor
  let text: UIColor
  let secondary: UIColor
  let accent: UIColor
  let separator: UIColor
  let bottomInset: CGFloat
}

@MainActor
protocol ZoenKeyboardPanelContent: AnyObject {
  var view: UIView { get }
  func update(model: [String: Any], appearance: ZoenKeyboardPanelAppearance) throws
  func retire()
}

/// One public UIKit inputView on the exact mounted RN editor. No second responder,
/// Fabric reparenting, delegate replacement, swizzle, keyboard timer or draft store.
@MainActor
class ZoenKeyboardPanelHost: ExpoView {
  let onAction = EventDispatcher()
  let onClosed = EventDispatcher()
  // Expo58 installs dispatchers from the concrete class's immediate Mirror children.
  // Leaf adapters provide their own named dispatchers and override these sinks.
  func emitActionEvent(_ value: [String: Any]) { onAction(value) }
  func emitClosedEvent(_ value: [String: Any]) { onClosed(value) }
  var binding = ""
  var model = "{}"
  var allowsTextEditingActions: Bool { false }
  func admitsInsertion(_ text: String) -> Bool { false }
  private weak var moduleContext: AppContext?
  private var owner: Owner?
  private var admissionRetired = false
  private var observers: [NSObjectProtocol] = []
  private static let hosts = NSHashTable<ZoenKeyboardPanelHost>.weakObjects()
  private static var admissionsEnabled = true
  private static var admissionEpoch = UUID()
  private var terminalTokens = Set<String>()
  private var waiters: [CheckedContinuation<Void, Never>] = []

  @MainActor private final class Owner {
    let token: String
    let context: String
    let tag: Int
    weak var editor: UITextView?
    let previousInput: UIView?
    let input: PanelInput
    let content: any ZoenKeyboardPanelContent
    var retiring = false
    var returnToKeyboard = false
    var reason = "closed"
    var sawWindow = false
    var keyboardGone = false
    init(token: String, context: String, tag: Int, editor: UITextView, input: PanelInput, content: any ZoenKeyboardPanelContent) {
      self.token = token; self.context = context; self.tag = tag
      self.editor = editor; self.previousInput = editor.inputView
      self.input = input; self.content = content
    }
  }

  required init(appContext: AppContext? = nil) {
    moduleContext = appContext
    super.init(appContext: appContext)
    isUserInteractionEnabled = false
    Self.hosts.add(self)
    let center = NotificationCenter.default
    for name in [UIResponder.keyboardWillChangeFrameNotification, UIResponder.keyboardDidChangeFrameNotification, UIResponder.keyboardDidHideNotification] {
      observers.append(center.addObserver(forName: name, object: nil, queue: .main) { [weak self] note in
        MainActor.assumeIsolated { self?.keyboard(note) }
      })
    }
    for name in [UIApplication.willResignActiveNotification, UIApplication.didEnterBackgroundNotification] {
      observers.append(center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
        MainActor.assumeIsolated { self?.close(reason: "inactive", returnToKeyboard: false) }
      })
    }
    observers.append(center.addObserver(forName: UITextView.textDidEndEditingNotification, object: nil, queue: .main) { [weak self] note in
      MainActor.assumeIsolated {
        guard let self, let editor = self.owner?.editor, note.object as? UITextView === editor else { return }
        self.close(reason: "focus", returnToKeyboard: true)
      }
    })
  }
  deinit { for observer in observers { NotificationCenter.default.removeObserver(observer) } }

  func makeContent(send: @escaping (String, [String: Any]) -> Void) -> any ZoenKeyboardPanelContent {
    ZoenAttachmentKeyboardContent(send: send)
  }
  override func didMoveToWindow() {
    super.didMoveToWindow()
    if window == nil { close(reason: "unmounted", returnToKeyboard: false) }
    else { applyProps() }
  }
  func applyProps() {
    guard Self.admissionsEnabled, !admissionRetired else { close(reason: "retired", returnToKeyboard: false); return }
    guard let requested = dictionary(binding, maximum: 8192),
          let active = requested["active"] as? Bool else {
      close(reason: "invalid", returnToKeyboard: false); return
    }
    if let owner {
      let same = requested["token"] as? String == owner.token && requested["context"] as? String == owner.context && requested["tag"] as? Int == owner.tag
      if !same || !active {
        close(reason: same ? "closed" : "replaced", returnToKeyboard: requested["returnToKeyboard"] as? Bool == true)
        return
      }
      guard !owner.retiring else { return }
      do {
        guard let value = dictionary(model, maximum: 4_194_304) else { throw Failure.invalid }
        try owner.content.update(model: value, appearance: appearance(requested, window: window))
      } catch { close(reason: "invalid", returnToKeyboard: false) }
      return
    }
    if !active {
      if requested["closing"] as? Bool == true { reject(requested, reason: "closed") }
      return
    }
    // Fabric can deliver the initial props before attaching this host. Keep
    // that presentation pending until didMoveToWindow retries; it is not a
    // terminal rejection, and session retirement still fences admission above.
    guard let window else { return }
    guard active,
          window.windowScene?.activationState == .foregroundActive,
          let tag = requested["tag"] as? Int, tag > 0,
          let token = requested["token"] as? String, !token.isEmpty, token.utf8.count <= 128,
          !terminalTokens.contains(token),
          let context = requested["context"] as? String, !context.isEmpty, context.utf8.count <= 2048,
          let label = requested["label"] as? String, !label.isEmpty,
          let root = moduleContext?.findView(withTag: tag, ofType: UIView.self), root.window === window,
          let editor = findEditor(root: root), editor.window === window, editor.isEditable,
          matchesLabel(root: root, editor: editor, label: label, placeholder: requested["placeholder"] as? String ?? ""),
          let value = dictionary(model, maximum: 4_194_304) else {
      if active { reject(requested, reason: "unavailable") }
      return
    }
    for other in Self.hosts.allObjects where other !== self && other.owner?.editor === editor {
      other.close(reason: "replaced", returnToKeyboard: true)
    }
    let content = makeContent { [weak self] action, payload in self?.send(action, payload: payload, token: token, context: context, tag: tag) }
    do { try content.update(model: value, appearance: appearance(requested, window: window)) }
    catch { content.retire(); reject(requested, reason: "invalid"); return }
    let observed = (requested["preferredHeight"] as? Double).flatMap { $0.isFinite && $0 > 0 ? CGFloat($0) : nil } ?? 0
    let height = observed > 0 ? min(observed, window.bounds.height * 0.5) : min(window.bounds.height * 0.5, 216 + window.safeAreaInsets.bottom)
    let input = PanelInput(height: max(162, height))
    input.backgroundColor = appearance(requested, window: window).background
    input.addSubview(content.view)
    let owner = Owner(token: token, context: context, tag: tag, editor: editor, input: input, content: content)
    self.owner = owner
    input.changedWindow = { [weak self, weak owner] in
      guard let self, let owner, self.owner === owner else { return }
      if owner.input.window != nil { owner.sawWindow = true }
      self.finishIfRemoved(owner)
    }
    editor.inputView = input
    if !editor.isFirstResponder && !editor.becomeFirstResponder() {
      close(reason: "unavailable", returnToKeyboard: false); return
    }
    editor.reloadInputViews()
  }

  private func send(_ action: String, payload: [String: Any], token: String, context: String, tag: Int) {
    guard let owner, owner.token == token, owner.context == context, owner.tag == tag, !owner.retiring, let editor = owner.editor, let window,
          editor.window === window, editor.isFirstResponder, editor.inputView === owner.input,
          window.windowScene?.activationState == .foregroundActive,
          let requested = dictionary(binding, maximum: 8192), requested["active"] as? Bool == true,
          requested["token"] as? String == owner.token, requested["context"] as? String == owner.context,
          requested["tag"] as? Int == owner.tag else { return }
    if action == "keyboard" {
      close(reason: "keyboard", returnToKeyboard: true)
      return
    }
    if action == "insert" {
      guard allowsTextEditingActions, let text = payload["text"] as? String,
            !text.isEmpty, text.utf16.count <= 128,
            admitsInsertion(text) else { return }
      editor.insertText(text)
      return
    }
    if action == "delete" {
      if allowsTextEditingActions { editor.deleteBackward() }
      return
    }
    var event: [String: Any] = ["token": owner.token, "context": owner.context, "action": action]
    if let id = payload["id"] as? String, id.utf8.count <= 2048 { event["id"] = id }
    if let revision = dictionary(model, maximum: 4_194_304)?["revision"] as? String { event["revision"] = revision }
    emitActionEvent(event)
  }
  private func close(reason: String, returnToKeyboard: Bool) {
    guard let owner else { resumeWaiters(); return }
    if owner.retiring { finishIfRemoved(owner); return }
    owner.retiring = true
    terminalTokens.insert(owner.token)
    owner.reason = reason
    owner.returnToKeyboard = returnToKeyboard
    owner.content.retire()
    if let editor = owner.editor, editor.inputView === owner.input {
      editor.inputView = owner.previousInput
      if returnToKeyboard { if editor.isFirstResponder { editor.reloadInputViews() } }
      else if editor.isFirstResponder { editor.resignFirstResponder() }
    }
    finishIfRemoved(owner)
  }
  private func finishIfRemoved(_ owner: Owner) {
    guard self.owner === owner, owner.retiring, owner.input.window == nil else { return }
    // Actual inputView removal is the handoff barrier. A pane that never entered
    // a window has nothing to dismiss. No completion timer or guessed duration.
    guard !owner.sawWindow || owner.returnToKeyboard || owner.keyboardGone else { return }
    self.owner = nil
    owner.input.changedWindow = nil
    owner.content.view.removeFromSuperview()
    emitClosedEvent(["token": owner.token, "context": owner.context, "reason": owner.reason])
    resumeWaiters()
    // A prop update may have replaced this owner while UIKit was dismissing it.
    applyProps()
  }
  private func keyboard(_ note: Notification) {
    let keyboardFrame = (note.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? NSValue)?.cgRectValue
    let height: CGFloat = if let keyboardFrame, let window {
      window.bounds.intersection(window.convert(keyboardFrame, from: nil)).isNull ? 0 : max(0, window.bounds.intersection(window.convert(keyboardFrame, from: nil)).height)
    } else { 0 }
    if let owner {
      if note.name == UIResponder.keyboardDidHideNotification || (note.name == UIResponder.keyboardDidChangeFrameNotification && height <= 0) {
        owner.keyboardGone = true
      }
      finishIfRemoved(owner)
    }
  }
  @discardableResult
  static func resumeAdmissions(expected: UUID) -> Bool {
    guard admissionEpoch == expected else { return false }
    admissionsEnabled = true
    return true
  }
  @discardableResult
  static func retireAll() async -> UUID {
    let epoch = UUID()
    admissionEpoch = epoch
    admissionsEnabled = false
    let current = hosts.allObjects
    // Fence ALL current hosts before the first suspension. No delayed props
    // may reopen a pane during account/device retirement.
    for host in current { host.admissionRetired = true; host.close(reason: "retired", returnToKeyboard: false) }
    for host in current { await host.closeAndWait() }
    return epoch
  }
  private func closeAndWait() async {
    guard owner != nil else { return }
    await withCheckedContinuation { continuation in
      waiters.append(continuation)
      close(reason: "retired", returnToKeyboard: false)
    }
  }
  private func resumeWaiters() {
    let values = waiters; waiters.removeAll()
    for value in values { value.resume() }
  }
  private func reject(_ requested: [String: Any], reason: String) {
    guard let token = requested["token"] as? String, let context = requested["context"] as? String else { return }
    guard !terminalTokens.contains(token) else { return }
    terminalTokens.insert(token)
    emitClosedEvent(["token": token, "context": context, "reason": reason])
  }
  private func dictionary(_ value: String, maximum: Int) -> [String: Any]? {
    guard let data = value.data(using: .utf8), data.count <= maximum else { return nil }
    return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
  }
  private func findEditor(root: UIView) -> UITextView? {
    var queue = [root], found: UITextView?, count = 0
    while let view = queue.popLast() {
      count += 1
      guard count <= 128 else { return nil }
      if let editor = view as? UITextView {
        guard found == nil else { return nil }
        found = editor
      }
      queue.append(contentsOf: view.subviews)
    }
    return found
  }
  private func matchesLabel(root: UIView, editor: UITextView, label: String, placeholder: String) -> Bool {
    if let actual = root.accessibilityLabel, !actual.isEmpty { return actual == label }
    let actual = editor.text.isEmpty && !placeholder.isEmpty ? "\(label) \(placeholder)" : label
    return editor.accessibilityLabel == actual
  }
  private func appearance(_ requested: [String: Any], window: UIWindow?) -> ZoenKeyboardPanelAppearance {
    let values = requested["appearance"] as? [String: String] ?? [:]
    return ZoenKeyboardPanelAppearance(background: color(values["background"], fallback: .systemBackground), text: color(values["text"], fallback: .label), secondary: color(values["secondary"], fallback: .secondaryLabel), accent: color(values["accent"], fallback: .systemBlue), separator: color(values["separator"], fallback: .separator), bottomInset: window?.safeAreaInsets.bottom ?? 0)
  }
  private func color(_ value: String?, fallback: UIColor) -> UIColor {
    guard let value, value.hasPrefix("#"), [7, 9].contains(value.count), let number = UInt64(value.dropFirst(), radix: 16) else { return fallback }
    let alpha = value.count == 9 ? CGFloat(number & 255) / 255 : 1
    let rgb = value.count == 9 ? number >> 8 : number
    return UIColor(red: CGFloat((rgb >> 16) & 255) / 255, green: CGFloat((rgb >> 8) & 255) / 255, blue: CGFloat(rgb & 255) / 255, alpha: alpha)
  }
  private enum Failure: Error { case invalid }
}

@MainActor
private final class PanelInput: UIInputView {
  var changedWindow: (() -> Void)?
  private let initialHeight: CGFloat
  private var height: CGFloat
  init(height: CGFloat) {
    self.initialHeight = height
    self.height = height
    super.init(frame: CGRect(x: 0, y: 0, width: 0, height: height), inputViewStyle: .keyboard)
    allowsSelfSizing = true
    autoresizingMask = [.flexibleWidth]
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
  override var intrinsicContentSize: CGSize { CGSize(width: UIView.noIntrinsicMetric, height: height) }
  override func layoutSubviews() {
    super.layoutSubviews()
    if let window {
      let next = max(162, min(initialHeight, window.bounds.height * 0.5))
      if abs(height - next) > 0.5 { height = next; invalidateIntrinsicContentSize() }
    }
    subviews.first?.frame = bounds
  }
  override func didMoveToWindow() { super.didMoveToWindow(); changedWindow?() }
}
