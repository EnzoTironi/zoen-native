import AVFoundation
import UIKit

/// Optional full-screen adapter. The gallery can embed `content` directly and
/// retain its existing caption/selection/footer instead. Both paths use exactly
/// the same original scrubber/player and explicit canonical Save closure.
@MainActor final class ZoenTelegramVideoEditingController: UIViewController {
  typealias Save = @MainActor (AVAsset, CMTimeRange, Bool) async throws -> Void
  let content: ZoenTelegramVideoEditingContent
  private let save: Save
  private let cancelled: () -> Void
  private let saved: () -> Void
  private let failed: (Error) -> Void
  private let cancelButton: UIControl
  private let commitButton: UIControl
  private let toolbar = UIView()
  private var saveTask: Task<Void, Never>?
  private var retired = false

  /// Controls are supplied by the host's shared Telegram toolbar factory. They
  /// retain their original glass, glyph and pressed-state implementations.
  init(content: ZoenTelegramVideoEditingContent, cancelControl: UIControl, commitControl: UIControl,
       save: @escaping Save, cancelled: @escaping () -> Void,
       saved: @escaping () -> Void, failed: @escaping (Error) -> Void) {
    self.content = content; self.save = save; self.cancelled = cancelled; self.saved = saved; self.failed = failed
    self.cancelButton = cancelControl; self.commitButton = commitControl
    super.init(nibName: nil, bundle: nil)
    modalPresentationStyle = .fullScreen
    cancelButton.accessibilityLabel = NSLocalizedString("Cancel", comment: "Video editor cancel")
    commitButton.accessibilityLabel = NSLocalizedString("Save", comment: "Video editor explicit save")
    cancelButton.addTarget(self, action: #selector(cancelPressed), for: .touchUpInside)
    commitButton.addTarget(self, action: #selector(savePressed), for: .touchUpInside)
  }
  required init?(coder: NSCoder) { nil }

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .black
    toolbar.backgroundColor = .clear
    view.addSubview(content.view); view.addSubview(content.headerView); view.addSubview(toolbar)
    toolbar.addSubview(cancelButton); toolbar.addSubview(commitButton)
  }
  override func viewDidLayoutSubviews() {
    super.viewDidLayoutSubviews()
    let insets = view.safeAreaInsets, bounds = view.bounds
    content.view.frame = bounds
    content.headerView.frame = CGRect(x: 0, y: insets.top, width: bounds.width, height: 64)
    // MediaPickerPhotoToolbarView.sideButtonFrames uses26pt and44×44 in
    // portrait, bottom/top controls on the corresponding side in landscape.
    if bounds.width > bounds.height {
      let left = view.window?.windowScene?.effectiveGeometry.interfaceOrientation == .landscapeLeft
      let width: CGFloat = 44 + (left ? insets.left : insets.right)
      toolbar.frame = CGRect(x: left ? 0 : bounds.width - width, y: 0, width: width, height: bounds.height)
      let x: CGFloat = left ? width - 44 : 0
      cancelButton.frame = CGRect(x: x, y: bounds.height - 44, width: 44, height: 44)
      commitButton.frame = CGRect(x: x, y: 0, width: 44, height: 44)
    } else {
      toolbar.frame = CGRect(x: 0, y: bounds.height - 44 - insets.bottom, width: bounds.width, height: 44 + insets.bottom)
      cancelButton.frame = CGRect(x: 26, y: 0, width: 44, height: 44)
      commitButton.frame = CGRect(x: bounds.width - 44 - 26, y: 0, width: 44, height: 44)
    }
    content.updateLayout(safeArea: insets)
  }
  override func viewDidAppear(_ animated: Bool) { super.viewDidAppear(animated); content.setVisible(true) }
  override func viewWillDisappear(_ animated: Bool) { super.viewWillDisappear(animated); content.setVisible(false) }
  override var prefersStatusBarHidden: Bool { true }
  override var supportedInterfaceOrientations: UIInterfaceOrientationMask { .allButUpsideDown }

  @objc private func cancelPressed() {
    guard !retired, saveTask == nil else { return }
    if !content.hasChanges { cancelled(); return }
    let alert = UIAlertController(title: NSLocalizedString("Discard changes?", comment: "Unsaved video edits"), message: nil, preferredStyle: .alert)
    alert.addAction(UIAlertAction(title: NSLocalizedString("Keep editing", comment: "Continue video editing"), style: .cancel))
    alert.addAction(UIAlertAction(title: NSLocalizedString("Discard", comment: "Discard unsaved video edits"), style: .destructive) { [weak self] _ in
      guard let self, !self.retired, self.saveTask == nil else { return }; self.cancelled()
    })
    present(alert, animated: true)
  }
  @objc private func savePressed() {
    guard !retired, saveTask == nil else { return }
    let state = content.state
    setBusy(true)
    saveTask = Task { @MainActor [weak self] in
      guard let self else { return }
      do {
        try Task.checkCancellation()
        try await self.save(self.content.asset, state.range, state.sendAsGif)
        guard !self.retired, !Task.isCancelled else { return }
        self.saveTask = nil; self.saved()
      } catch {
        guard !self.retired, !Task.isCancelled else { return }
        self.saveTask = nil; self.setBusy(false); self.failed(error)
      }
    }
  }
  private func setBusy(_ busy: Bool) {
    cancelButton.isEnabled = !busy; commitButton.isEnabled = !busy
    content.view.isUserInteractionEnabled = !busy; content.headerView.isUserInteractionEnabled = !busy
    content.setVisible(!busy && view.window != nil)
  }
  func retire(sensitive: Bool) {
    guard !retired else { return }
    retired = true; saveTask?.cancel(); saveTask = nil
    content.retire(sensitive: sensitive)
    if sensitive { viewIfLoaded?.isHidden = true }
  }
}
