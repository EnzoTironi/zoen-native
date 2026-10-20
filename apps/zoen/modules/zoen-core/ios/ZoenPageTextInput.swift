import ExpoModulesCore
import Foundation
import UIKit

/// UIKit owns live text, IME and ranges. Only the encrypted Rust journal retains it.
@MainActor final class ZoenPageTextInput: ExpoView, UITextViewDelegate {
  private let textView = ZoenPageEditorTextView()
  private var owner: ZoenPageInputProducer?
  private var sequence: UInt64 = 0
  private var prior = ""
  private var spans: [[String: Any]] = []
  private var pending: Data?
  private var sealed = false
  private var restoring = false
  private var mounted = false
  private var boundToken: String?
  var token = ""

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    textView.delegate = self
    textView.writingToolsBehavior = .none
    textView.admission = { [weak self] in self?.mayMutate() == true }
    textView.checkpoint = { [weak self] in self?.captureIfLive() }
    textView.isEditable = false
    textView.font = .preferredFont(forTextStyle: .body)
    textView.adjustsFontForContentSizeCategory = true
    textView.backgroundColor = .secondarySystemGroupedBackground
    textView.textColor = .label
    textView.textContainerInset = UIEdgeInsets(top: 18, left: 14, bottom: 18, right: 14)
    textView.layer.cornerRadius = 18
    addSubview(textView)
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    textView.frame = bounds
  }

  func bind() {
    guard !token.isEmpty else { return }
    if let owner {
      if boundToken == token { return }
      let status = owner.lease.status()
      guard !status.valid || status.completed else {
        owner.lease.draftCheckpointFailed(ZoenPageInputLease.Failure.changed)
        return
      }
      self.owner = nil
      sequence = 0; prior = ""; spans = []; pending = nil; sealed = false
    }
    do {
      let producer = try ZoenPageInputProducer.claimText(token: token, view: self)
      owner = producer
      boundToken = token
      let payload = try producer.lease.initialSnapshot()
      restoring = true
      defer { restoring = false }
      if let payload {
        let value = try JSONSerialization.jsonObject(with: payload) as? [String: Any]
        guard let value, value["schema"] as? String == "page-text/1",
              value["mode"] as? String == producer.mode,
              let text = value["text"] as? String,
              let number = value["sequence"] as? String, let restored = UInt64(number),
              let selection = value["selection"] as? [String: Int],
              let start = selection["start"], let end = selection["end"],
              start >= 0, start <= end, end <= text.utf16.count else {
          throw ZoenPageInputLease.Failure.invalidCapture
        }
        sequence = restored
        textView.text = text
        spans = value["spans"] as? [[String: Any]] ?? []
        textView.selectedRange = NSRange(location: start, length: end - start)
        // Restored composition is ordinary text. Never invent an IME candidate session.
      } else {
        textView.text = producer.initialText
        spans = producer.initialSpans
      }
      prior = textView.text
      if producer.mode == "markdown" { textView.font = .monospacedSystemFont(ofSize: 17, weight: .regular) }
      textView.accessibilityLabel = producer.label
      textView.isEditable = true
      if window != nil { textView.becomeFirstResponder() }
    } catch {
      owner?.lease.draftCheckpointFailed(error)
    }
  }

  override func didMoveToWindow() {
    super.didMoveToWindow()
    if window != nil {
      mounted = true
      if owner != nil, !sealed { textView.becomeFirstResponder() }
    }
  }

  override func willMove(toWindow newWindow: UIWindow?) {
    // A guarded route leave seals before removal. This is also a final native
    // checkpoint if React disappears without delivering another JS callback.
    if newWindow == nil, mounted, owner != nil, !sealed { try? seal() }
    super.willMove(toWindow: newWindow)
  }

  func textView(_ textView: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
    mayMutate()
  }

  private func mayMutate() -> Bool {
    guard let owner else { return false }
    let status = owner.lease.status()
    return !sealed && !status.failed && status.valid && !status.completed
  }

  func textViewDidChange(_ textView: UITextView) { captureIfLive() }
  func textViewDidChangeSelection(_ textView: UITextView) { captureIfLive() }

  private func captureIfLive() {
    guard !restoring, textView.nativeDepth == 0, !sealed, let owner, !owner.lease.status().failed else { return }
    do { try checkpoint() }
    catch { owner.lease.draftCheckpointFailed(error) }
  }

  private func checkpoint() throws {
    guard let owner, owner.lease.status().valid else { throw ZoenPageInputLease.Failure.closed }
    if let pending {
      _ = try owner.lease.retainDraftSnapshot(pending, sequence: sequence)
      self.pending = nil
    }
    guard sequence < UInt64.max else { throw ZoenPageInputLease.Failure.invalidCapture }
    let text = textView.text ?? ""
    if owner.mode == "lossless-text" { spans = Self.retarget(before: prior, after: text, spans: spans) }
    prior = text
    let selection = textView.selectedRange
    var composition: Any = NSNull()
    if let marked = textView.markedTextRange {
      let start = textView.offset(from: textView.beginningOfDocument, to: marked.start)
      let end = textView.offset(from: textView.beginningOfDocument, to: marked.end)
      composition = ["start": start, "end": end]
    }
    sequence += 1
    var value: [String: Any] = ["schema": "page-text/1", "sequence": String(sequence),
      "mode": owner.mode, "text": text,
      "selection": ["start": selection.location, "end": NSMaxRange(selection)], "composition": composition]
    if owner.mode == "lossless-text" { value["spans"] = spans }
    let payload = try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
    guard payload.count <= 1_048_576 else { throw ZoenPageInputLease.Failure.invalidCapture }
    pending = payload
    _ = try owner.lease.retainDraftSnapshot(payload, sequence: sequence)
    pending = nil
  }

  func flush() throws { try checkpoint() }

  func seal() throws {
    // Do not blur, toggle editable, or unmark live composition to get a receipt.
    sealed = true
    try checkpoint()
  }

  func freeze() { sealed = true }

  /// Same one-replacement policy as documents/editing.ts, in UTF-16 scalar boundaries.
  static func retarget(before: String, after: String, spans: [[String: Any]]) -> [[String: Any]] {
    let old = Array(before.utf16), new = Array(after.utf16)
    if old == new { return spans }
    var start = 0
    while start < old.count && start < new.count && old[start] == new[start] { start += 1 }
    if start > 0 && (0xD800...0xDBFF).contains(old[start - 1]) { start -= 1 }
    var oldEnd = old.count, newEnd = new.count
    while oldEnd > start && newEnd > start && old[oldEnd - 1] == new[newEnd - 1] { oldEnd -= 1; newEnd -= 1 }
    if oldEnd < old.count && (0xDC00...0xDFFF).contains(old[oldEnd]) { oldEnd += 1; newEnd += 1 }
    let delta = newEnd - oldEnd
    return spans.compactMap { span in
      guard let from = span["start"] as? Int, let to = span["end"] as? Int else { return nil }
      let nextFrom = from < start ? from : from >= oldEnd ? from + delta : start
      let nextTo = to <= start ? to : to >= oldEnd ? to + delta : newEnd
      guard nextFrom < nextTo else { return nil }
      var result = span
      result["start"] = nextFrom; result["end"] = nextTo
      return result
    }
  }
}

/// Native IME/keyboard entry points are fenced before mutation, and checkpointed
/// after their outermost synchronous change. Delegate callbacks inside an IME
/// operation cannot retain an intermediate text/selection pair.
@MainActor private final class ZoenPageEditorTextView: UITextView {
  var admission: (() -> Bool)?
  var checkpoint: (() -> Void)?
  private(set) var nativeDepth = 0
  private func mutate(_ work: () -> Void) {
    guard admission?() == true else { return }
    nativeDepth += 1
    work()
    nativeDepth -= 1
    if nativeDepth == 0 { checkpoint?() }
  }
  override func insertText(_ text: String) { mutate { super.insertText(text) } }
  override func deleteBackward() { mutate { super.deleteBackward() } }
  override func setMarkedText(_ markedText: String?, selectedRange: NSRange) {
    mutate { super.setMarkedText(markedText, selectedRange: selectedRange) }
  }
  override func unmarkText() { mutate { super.unmarkText() } }
  override func replace(_ range: UITextRange, withText text: String) {
    mutate { super.replace(range, withText: text) }
  }
}
