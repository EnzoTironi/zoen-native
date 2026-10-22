import ExpoModulesCore
import UIKit

@MainActor extension ZoenKeyboardPanelHost {
  func send(_ action: String, payload: [String: Any], token: String, context: String, tag: Int) {
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
    if action == "option" {
      guard owner.attachment != nil, let option = payload["id"] as? String,
            ["send-as-file", "send-without-grouping"].contains(option),
            option != "send-without-grouping" || ((payload["ids"] as? [String])?.count ?? 0) >= 2,
            let supplied = payload["revision"] as? String, supplied == currentRevision else { return }
    }
    if action == "reorder" || action == "option" {
      guard owner.attachment != nil, let ids = payload["ids"] as? [String],
            action == "option" ? (1...10).contains(ids.count) : ids.count <= 100,
            Set(ids).count == ids.count,
            ids.allSatisfy({ !$0.isEmpty && $0.utf8.count <= 2048 && !$0.contains("\0") }) else { return }
      event["ids"] = ids
    }
    if action == "caption" || action == "send" || action == "edit" || action == "recentSend" || action == "recentQuery" || action == "option" {
      guard owner.attachment != nil, let text = payload["text"] as? String,
            text.utf16.count <= (action == "recentQuery" ? 512 : 4096), !text.contains("\0") else { return }
      event["text"] = text
    }
    if let currentRevision { event["revision"] = currentRevision }
    emitActionEvent(event)
  }
  func close(reason: String, returnToKeyboard: Bool) {
    guard let owner else { resumeWaiters(); return }
    if reason != "closed" && reason != "keyboard" {
      // A sensitive close may overtake an ordinary animated close or its camera
      // drain. Conceal immediately even when action authority is already retired.
      owner.content.view.isHidden = true
    }
    if owner.retiring { finishIfRemoved(owner); return }
    owner.retiring = true
    terminalTokens.insert(owner.token)
    owner.reason = reason
    owner.returnToKeyboard = returnToKeyboard
    if let attachment = owner.attachment {
      guard let content = owner.content as? any ZoenAttachmentOverlayContent else { return }
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
  func finishIfRemoved(_ owner: Owner) {
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
}
