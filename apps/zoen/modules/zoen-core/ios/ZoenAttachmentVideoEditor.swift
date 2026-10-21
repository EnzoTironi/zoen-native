import AVFoundation
import UIKit

/// Thin presentation adapter for the pinned original Telegram scrubber/player.
/// FilePreview alone owns output, authorization, CAS, recovery and dismissal.
@MainActor final class ZoenAttachmentVideoEditor: NSObject, ZoenFilePreviewPresentation {
  private weak var presenter: UIViewController?
  private let editing: ZoenFilePreview.Editing
  private var editor: ZoenTelegramVideoEditingController?
  private var preparation: Task<Void, Never>?
  private var ended: (@MainActor (Error?) -> Void)?
  private var presenting = false
  private var dismissing = false
  private var waiting = false
  private var retired = false
  private var callbacks: [@MainActor () -> Void] = []
  private var recovery: UIAlertController?

  init(presenter: UIViewController?, editing: ZoenFilePreview.Editing) {
    self.presenter = presenter; self.editing = editing
  }
  func show(file: URL, title: String, ended: @escaping @MainActor (Error?) -> Void) throws {
    guard editing.allowsEditing, file.isFileURL, preparation == nil, editor == nil, !retired,
          let presenter, presenter.viewIfLoaded?.window != nil,
          !presenter.isBeingPresented, !presenter.isBeingDismissed,
          presenter.presentedViewController == nil else { throw ZoenFilePreview.Failure.unavailable }
    self.ended = ended
    preparation = Task { @MainActor [weak self] in
      guard let self else { return }
      defer { preparation = nil; drainDismissal() }
      do {
        let asset = AVURLAsset(url: file)
        let glyphs = try ZoenTelegramVideoGlyphs.bundled()
        let content = try await ZoenTelegramVideoEditingContent.prepare(asset: asset,
          allowsTrimming: true, glyphs: glyphs)
        guard !retired, !Task.isCancelled, callbacks.isEmpty,
              self.presenter === presenter, presenter.viewIfLoaded?.window != nil,
              presenter.presentedViewController == nil else {
          content.retire(sensitive: true); return
        }
        let controller = ZoenTelegramVideoEditingController(content: content,
          cancelControl: VideoEditorButton(glyph: .close), commitControl: VideoEditorButton(glyph: .done),
          save: { [weak self] asset, range, muted in
            guard let self, !self.retired else { throw ZoenFilePreview.Failure.changed }
            try await self.editing.receiveVideo { destination, maximumBytes in
              _ = try await ZoenTelegramVideoRenderer.render(asset: asset, range: range,
                sendAsGif: muted, destination: destination, maximumBytes: maximumBytes)
            }
          }, cancelled: { [weak self] in self?.userClose() },
          saved: { [weak self] in self?.userClose() },
          failed: { [weak self] in self?.show(error: $0) })
        content.failed = { [weak self] in self?.show(error: $0) }
        controller.title = title
        editor = controller; presenting = true
        presenter.present(controller, animated: !UIAccessibility.isReduceMotionEnabled) { [weak self] in
          guard let self else { return }; presenting = false; drainDismissal()
        }
      } catch {
        guard !retired, !Task.isCancelled else { return }
        let callback = self.ended; self.ended = nil; callback?(error)
      }
    }
  }
  private func userClose() {
    dismiss { [weak self] in
      guard let self else { return }
      let callback = ended; ended = nil; callback?(nil)
    }
  }
  func dismiss(completion: @escaping @MainActor () -> Void) {
    retired = true; callbacks.append(completion)
    preparation?.cancel(); editor?.retire(sensitive: true)
    drainDismissal()
  }
  private func drainDismissal() {
    guard preparation == nil, !presenting, !dismissing, !waiting, !callbacks.isEmpty else { return }
    let active: UIViewController? = recovery?.presentingViewController != nil ? recovery : editor
    if let transition = active?.transitionCoordinator ?? editor?.presentedViewController?.transitionCoordinator {
      waiting = true
      if transition.animate(alongsideTransition: nil, completion: { [weak self] _ in
        Task { @MainActor [weak self] in self?.waiting = false; self?.drainDismissal() }
      }) { return }
      waiting = false
    }
    dismissing = true
    if let parent = editor?.presentingViewController ?? recovery?.presentingViewController {
      parent.dismiss(animated: false) { [weak self] in self?.finishDismissal() }
    } else { finishDismissal() }
  }
  private func finishDismissal() {
    editor = nil; recovery = nil; dismissing = false
    let pending = callbacks; callbacks = []; pending.forEach { $0() }
  }
  private func show(error: Error) {
    guard !retired, let editor, editor.presentedViewController == nil else { return }
    let alert = UIAlertController(title: NSLocalizedString("Unable to save changes", comment: "Video edit failure"),
      message: error.localizedDescription, preferredStyle: .alert)
    alert.addAction(UIAlertAction(title: NSLocalizedString("OK", comment: "Acknowledge video edit error"), style: .default))
    editor.present(alert, animated: !UIAccessibility.isReduceMotionEnabled)
  }
  func captureFailed(error: Error) { show(error: error) }
  func recover(error: Error, changesSaved: Bool, retry: (@MainActor () -> Void)?, discard: @escaping @MainActor () -> Void) {
    recover(error: error, changesSaved: changesSaved, retry: retry, keep: nil, discard: discard)
  }
  func recover(error: Error, changesSaved: Bool, retry: (@MainActor () -> Void)?, keep: (@MainActor () -> Void)?, discard: @escaping @MainActor () -> Void) {
    guard recovery == nil, let presenter, presenter.viewIfLoaded?.window != nil else { return }
    let alert = UIAlertController(title: NSLocalizedString("Your changes need attention", comment: "Media edit recovery"),
      message: error.localizedDescription, preferredStyle: .alert)
    if let retry { alert.addAction(UIAlertAction(title: NSLocalizedString("Try Again", comment: "Retry media save"), style: .default) { [weak self] _ in self?.recovery = nil; retry() }) }
    if let keep { alert.addAction(UIAlertAction(title: NSLocalizedString("Keep for Later", comment: "Retain private media edit"), style: .default) { [weak self] _ in self?.recovery = nil; keep() }) }
    alert.addAction(UIAlertAction(title: changesSaved ? NSLocalizedString("Close", comment: "Close saved media") : NSLocalizedString("Discard Changes", comment: "Discard unsaved media edit"), style: .destructive) { [weak self] _ in self?.recovery = nil; discard() })
    recovery = alert; presenter.present(alert, animated: !UIAccessibility.isReduceMotionEnabled)
  }
}

/// Same shared original glass and original glyphs as the Photos gallery. This
/// supplies only the controls required by PhotoToolbar's original side frames.
@MainActor private final class VideoEditorButton: UIButton {
  private let glass = ZoenTelegramAttachmentGlassView()
  init(glyph: ZoenTelegramAttachmentGlyph) {
    super.init(frame: .zero)
    glass.isUserInteractionEnabled = false; insertSubview(glass, at: 0)
    setImage(ZoenTelegramAttachmentGlyph.image(glyph, pointSize: CGSize(width: 28, height: 28)), for: .normal)
    tintColor = .white
  }
  required init?(coder: NSCoder) { nil }
  override func layoutSubviews() {
    super.layoutSubviews(); glass.frame = bounds
    glass.update(isDark: true, cornerRadius: 22, isInteractive: true)
    if let imageView { bringSubviewToFront(imageView) }
  }
}
