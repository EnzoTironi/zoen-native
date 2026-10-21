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
  var isDark: Bool {
    var red: CGFloat = 0, green: CGFloat = 0, blue: CGFloat = 0, alpha: CGFloat = 0
    guard background.getRed(&red, green: &green, blue: &blue, alpha: &alpha) else { return false }
    return red * 0.2126 + green * 0.7152 + blue * 0.0722 < 0.5
  }
}

@MainActor
protocol ZoenKeyboardPanelContent: AnyObject {
  var view: UIView { get }
  func update(model: [String: Any], appearance: ZoenKeyboardPanelAppearance) throws
  func retire()
}

/// The exact mounted editor owns both surfaces. Emoji uses its public inputView;
/// attachments use an admitted sibling overlay. Neither reparents a Fabric view,
/// replaces a delegate, swizzles UIKit, or keeps a second draft store.
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
  var requiresMountedEditor: Bool { true }
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
    let input: PanelInput?
    let attachment: ZoenAttachmentContainer?
    let content: any ZoenKeyboardPanelContent
    var retiring = false
    var returnToKeyboard = false
    var reason = "closed"
    var sawWindow = false
    var keyboardGone = false
    var releaseParentGallery: (@MainActor @Sendable () -> Void)?
    init(token: String, context: String, tag: Int, editor: UITextView?, input: PanelInput?, attachment: ZoenAttachmentContainer?, content: any ZoenKeyboardPanelContent) {
      self.token = token; self.context = context; self.tag = tag
      self.editor = editor; self.previousInput = editor?.inputView
      self.input = input; self.attachment = attachment; self.content = content
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
        // Opening this exact admitted overlay deliberately resigns the composer.
        // It keeps its token/tag/context, not arbitrary first-responder authority.
        if self.owner?.attachment != nil { return }
        self.close(reason: "focus", returnToKeyboard: true)
      }
    })
    for name in [UITextView.textDidBeginEditingNotification, UITextField.textDidBeginEditingNotification] {
      observers.append(center.addObserver(forName: name, object: nil, queue: .main) { [weak self] note in
        MainActor.assumeIsolated {
          guard let self, let owner = self.owner, let attachment = owner.attachment,
                let field = note.object as? UIView else { return }
          if field !== owner.editor && !field.isDescendant(of: attachment) {
            self.close(reason: "focus", returnToKeyboard: false)
          }
        }
      })
    }
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
      let same = requested["token"] as? String == owner.token && requested["context"] as? String == owner.context
        && (requiresMountedEditor ? requested["tag"] as? Int == owner.tag : requested["standalone"] as? Bool == true && requested["tag"] == nil)
      if !same || !active {
        close(reason: same ? "closed" : "replaced", returnToKeyboard: requested["returnToKeyboard"] as? Bool == true)
        return
      }
      guard !owner.retiring else { return }
      do {
        guard let value = dictionary(model, maximum: 4_194_304) else { throw Failure.invalid }
        try owner.content.update(model: value, appearance: appearance(requested, window: window))
        owner.attachment?.updateAppearance(appearance(requested, window: window))
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
          let token = requested["token"] as? String, !token.isEmpty, token.utf8.count <= 128,
          !terminalTokens.contains(token),
          let context = requested["context"] as? String, !context.isEmpty, context.utf8.count <= 2048,
          let value = dictionary(model, maximum: 4_194_304) else {
      if active { reject(requested, reason: "unavailable") }
      return
    }
    let tag: Int
    let editor: UITextView?
    if requiresMountedEditor {
      // Mounted admission keeps its exact Fabric tag, label, editable editor and
      // window checks. The standalone subclass cannot access this editor path.
      guard let mountedTag = requested["tag"] as? Int, mountedTag > 0,
            let label = requested["label"] as? String, !label.isEmpty,
            let root = moduleContext?.findView(withTag: mountedTag, ofType: UIView.self), root.window === window,
            let mountedEditor = findEditor(root: root), mountedEditor.window === window, mountedEditor.isEditable,
            matchesLabel(root: root, editor: mountedEditor, label: label, placeholder: requested["placeholder"] as? String ?? "") else {
        reject(requested, reason: "unavailable"); return
      }
      tag = mountedTag; editor = mountedEditor
    } else {
      guard requested["standalone"] as? Bool == true, requested["tag"] == nil,
            !allowsTextEditingActions, window.rootViewController?.view.window === window else {
        reject(requested, reason: "unavailable"); return
      }
      tag = 0; editor = nil
    }
    for other in Self.hosts.allObjects where other !== self {
      if let editor, other.owner?.editor === editor {
        other.close(reason: "replaced", returnToKeyboard: true)
      } else if !requiresMountedEditor && !other.requiresMountedEditor && other.window === window {
        other.close(reason: "replaced", returnToKeyboard: false)
      }
    }
    let content = makeContent { [weak self] action, payload in self?.send(action, payload: payload, token: token, context: context, tag: tag) }
    do { try content.update(model: value, appearance: appearance(requested, window: window)) }
    catch { content.retire(); reject(requested, reason: "invalid"); return }
    let observed = (requested["preferredHeight"] as? Double).flatMap { $0.isFinite && $0 > 0 ? CGFloat($0) : nil } ?? 0
    let height = observed > 0 ? min(observed, window.bounds.height * 0.5) : min(window.bounds.height * 0.5, 216 + window.safeAreaInsets.bottom)
    let input = allowsTextEditingActions ? PanelInput(height: max(162, height), content: content.view) : nil
    input?.backgroundColor = appearance(requested, window: window).background
    let attachment: ZoenAttachmentContainer?
    if input == nil, let overlayContent = content as? any ZoenAttachmentOverlayContent {
      attachment = ZoenAttachmentContainer(content: overlayContent, appearance: appearance(requested, window: window)) { [weak self] in
        self?.close(reason: "closed", returnToKeyboard: false)
      }
    } else { attachment = nil }
    guard input != nil || attachment != nil else { content.retire(); reject(requested, reason: "unavailable"); return }
    let owner = Owner(token: token, context: context, tag: tag, editor: editor, input: input, attachment: attachment, content: content)
    self.owner = owner
    if let attachment {
      if !requiresMountedEditor { owner.releaseParentGallery = ZoenAttachmentGalleryHost.yieldForChildPresentation() }
      if editor?.isFirstResponder == true { editor?.resignFirstResponder() }
      attachment.present(in: window)
      return
    }
    guard let input, let editor else { close(reason: "unavailable", returnToKeyboard: false); return }
    input.changedWindow = { [weak self, weak owner] in
      guard let self, let owner, self.owner === owner else { return }
      if owner.input?.window != nil { owner.sawWindow = true }
      self.finishIfRemoved(owner)
    }
    editor.inputView = input
    if !editor.isFirstResponder && !editor.becomeFirstResponder() {
      close(reason: "unavailable", returnToKeyboard: false); return
    }
    editor.reloadInputViews()
  }

  private func send(_ action: String, payload: [String: Any], token: String, context: String, tag: Int) {
    guard let owner, owner.token == token, owner.context == context, owner.tag == tag, !owner.retiring, let window,
          window.windowScene?.activationState == .foregroundActive,
          let requested = dictionary(binding, maximum: 8192), requested["active"] as? Bool == true,
          requested["token"] as? String == owner.token, requested["context"] as? String == owner.context,
          (requiresMountedEditor ? requested["tag"] as? Int == owner.tag : requested["standalone"] as? Bool == true) else { return }
    if requiresMountedEditor {
      guard let editor = owner.editor, editor.window === window else { return }
    } else if owner.editor != nil || requested["tag"] != nil { return }
    if let attachment = owner.attachment {
      guard attachment.window === window else { return }
    } else {
      guard let editor = owner.editor, editor.isFirstResponder, let input = owner.input, editor.inputView === input else { return }
    }
    if action == "keyboard" {
      close(reason: "keyboard", returnToKeyboard: true)
      return
    }
    if action == "close" { close(reason: "closed", returnToKeyboard: false); return }
    if action == "insert" {
      guard requiresMountedEditor, allowsTextEditingActions, let editor = owner.editor, let text = payload["text"] as? String,
            !text.isEmpty, text.utf16.count <= 128,
            admitsInsertion(text) else { return }
      editor.insertText(text)
      return
    }
    if action == "delete" {
      if requiresMountedEditor && allowsTextEditingActions { owner.editor?.deleteBackward() }
      return
    }
    let currentRevision = dictionary(model, maximum: 4_194_304)?["revision"] as? String
    if payload["revision"] != nil {
      guard let supplied = payload["revision"] as? String, supplied == currentRevision else { return }
    }
    var event: [String: Any] = ["token": owner.token, "context": owner.context, "action": action]
    if let id = payload["id"] as? String, id.utf8.count <= 2048, !id.contains("\0") { event["id"] = id }
    if action == "reorder" {
      guard owner.attachment != nil, let ids = payload["ids"] as? [String], ids.count <= 100,
            Set(ids).count == ids.count,
            ids.allSatisfy({ !$0.isEmpty && $0.utf8.count <= 2048 && !$0.contains("\0") }) else { return }
      event["ids"] = ids
    }
    if action == "caption" || action == "send" || action == "edit" {
      guard owner.attachment != nil, let text = payload["text"] as? String, text.utf16.count <= 4096, !text.contains("\0") else { return }
      event["text"] = text
    }
    if let currentRevision { event["revision"] = currentRevision }
    emitActionEvent(event)
  }
  private func close(reason: String, returnToKeyboard: Bool) {
    guard let owner else { resumeWaiters(); return }
    if owner.retiring { finishIfRemoved(owner); return }
    owner.retiring = true
    terminalTokens.insert(owner.token)
    owner.reason = reason
    owner.returnToKeyboard = returnToKeyboard
    if let attachment = owner.attachment {
      guard let content = owner.content as? any ZoenAttachmentOverlayContent else { return }
      if reason != "closed" && reason != "keyboard" {
        // Hide native pixels immediately on background/context retirement. The
        // camera/player stop and actual removal still precede closed/erasure.
        content.view.isHidden = true
      }
      content.retireForDismissal { [weak self, weak owner] in
        guard let self, let owner, self.owner === owner else { return }
        attachment.dismiss(animated: reason != "inactive" && reason != "retired" && reason != "unmounted") { [weak self, weak owner] in
          guard let self, let owner, self.owner === owner else { return }
          if owner.returnToKeyboard, let editor = owner.editor, editor.window === self.window,
             self.window?.windowScene?.activationState == .foregroundActive {
            editor.becomeFirstResponder()
          }
          self.finishIfRemoved(owner)
        }
      }
      return
    }
    owner.content.retire()
    if let input = owner.input, let editor = owner.editor, editor.inputView === input {
      editor.inputView = owner.previousInput
      if returnToKeyboard { if editor.isFirstResponder { editor.reloadInputViews() } }
      else if editor.isFirstResponder { editor.resignFirstResponder() }
    }
    finishIfRemoved(owner)
  }
  private func finishIfRemoved(_ owner: Owner) {
    guard self.owner === owner, owner.retiring else { return }
    if let attachment = owner.attachment {
      guard attachment.superview == nil else { return }
    } else {
      guard owner.input?.window == nil else { return }
      // Actual inputView removal is the handoff barrier. A pane that never entered
      // a window has nothing to dismiss. No completion timer or guessed duration.
      guard !owner.sawWindow || owner.returnToKeyboard || owner.keyboardGone else { return }
    }
    self.owner = nil
    owner.input?.changedWindow = nil
    owner.content.view.removeFromSuperview()
    owner.releaseParentGallery?(); owner.releaseParentGallery = nil
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
      if let attachment = owner.attachment, !owner.retiring {
        if height <= 0 || (owner.content as? any ZoenAttachmentOverlayContent)?.attachmentHasKeyboardFocus == true {
          let duration = note.userInfo?[UIResponder.keyboardAnimationDurationUserInfoKey] as? Double ?? 0.25
          let rawCurve = note.userInfo?[UIResponder.keyboardAnimationCurveUserInfoKey] as? Int ?? 0
          attachment.keyboardChanged(height: height, duration: duration, curve: UIView.AnimationCurve(rawValue: rawCurve) ?? .easeInOut)
        }
      }
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

/// The same Photos/container implementation for Add from an admitted gallery.
/// It has no editor tag or insertion capability and shares the base retirement
/// registry, terminal-token checks and actual close-before-release barrier.
@MainActor
final class ZoenStandaloneAttachmentPanelHost: ZoenKeyboardPanelHost {
  private let standaloneActionEvent = EventDispatcher("onAction")
  private let standaloneClosedEvent = EventDispatcher("onClosed")
  override var requiresMountedEditor: Bool { false }
  override func emitActionEvent(_ value: [String: Any]) { standaloneActionEvent(value) }
  override func emitClosedEvent(_ value: [String: Any]) { standaloneClosedEvent(value) }
}

@MainActor
private final class PanelInput: UIInputView {
  var changedWindow: (() -> Void)?
  private let initialHeight: CGFloat
  private var height: CGFloat
  private let content: UIView
  init(height: CGFloat, content: UIView) {
    self.initialHeight = height
    self.height = height
    self.content = content
    super.init(frame: CGRect(x: 0, y: 0, width: 0, height: height), inputViewStyle: .keyboard)
    allowsSelfSizing = true
    autoresizingMask = [.flexibleWidth]
    addSubview(content)
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
  override var intrinsicContentSize: CGSize { CGSize(width: UIView.noIntrinsicMetric, height: height) }
  override func layoutSubviews() {
    super.layoutSubviews()
    if let window {
      let next = max(162, min(initialHeight, window.bounds.height * 0.5))
      if abs(height - next) > 0.5 { height = next; invalidateIntrinsicContentSize() }
    }
    // UIInputView can insert its own background before our content.
    content.frame = bounds
  }
  override func didMoveToWindow() { super.didMoveToWindow(); changedWindow?() }
}
