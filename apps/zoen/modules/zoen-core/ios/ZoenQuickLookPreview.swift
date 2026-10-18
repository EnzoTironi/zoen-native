import Foundation
import QuickLook
import UIKit

/// Public UIKit/Quick Look adapter. No Expo presentation Promise or row owns the
/// viewer's lifetime, and no editable copy is offered without a canonical save.
@MainActor final class ZoenQuickLookPreview: NSObject, ZoenFilePreviewPresentation,
  QLPreviewControllerDataSource, @MainActor QLPreviewControllerDelegate {
  private final class Item: NSObject, QLPreviewItem {
    let previewItemURL: URL?
    let previewItemTitle: String?
    init(file: URL, title: String) { previewItemURL = file; previewItemTitle = title }
  }

  private final class Controller: QLPreviewController {
    var disappeared: (() -> Void)?
    var appeared: (() -> Void)?
    override func viewDidAppear(_ animated: Bool) {
      super.viewDidAppear(animated)
      appeared?()
    }
    override func viewDidDisappear(_ animated: Bool) {
      super.viewDidDisappear(animated)
      // A full-screen Share child may cover Quick Look without closing it.
      if isBeingDismissed || presentingViewController == nil { disappeared?() }
    }
  }

  private weak var presenter: UIViewController?
  private weak var window: UIWindow?
  private let editing: ZoenFilePreview.Editing?
  private var controller: Controller?
  private var item: Item?
  private var ended: (@MainActor (Error?) -> Void)?
  private var didEnd = false
  private var presenting = false
  private var dismissing = false
  private var waitingForTransition = false
  private var dismissal: [@MainActor () -> Void] = []
  private var recovery: UIAlertController?
  private var recoveryID = UUID()

  init(presenter: UIViewController?, editing: ZoenFilePreview.Editing? = nil) {
    self.presenter = presenter
    self.editing = editing
  }

  func show(file: URL, title: String, ended: @escaping @MainActor (Error?) -> Void) throws {
    guard controller == nil, let presenter,
          presenter.viewIfLoaded?.window != nil,
          !presenter.isBeingPresented, !presenter.isBeingDismissed,
          presenter.transitionCoordinator == nil, presenter.presentedViewController == nil,
          QLPreviewController.canPreview(file as NSURL) else { throw ZoenFilePreview.Failure.unavailable }
    let controller = Controller()
    window = presenter.viewIfLoaded?.window
    self.controller = controller
    self.item = Item(file: file, title: title)
    self.ended = ended
    controller.dataSource = self
    controller.delegate = self
    controller.disappeared = { [weak self] in self?.notifyEnded(nil) }
    controller.appeared = { [weak self] in
      guard let self, self.waitingForTransition else { return }
      self.retryDismissal()
    }
    presenting = true
    presenter.present(controller, animated: true) { [self] in
      presenting = false
      if controller.presentingViewController == nil { notifyEnded(ZoenFilePreview.Failure.unavailable) }
      if !dismissal.isEmpty { dismissHierarchy() }
    }
  }

  func dismiss(completion: @escaping @MainActor () -> Void) {
    recoveryID = UUID()
    dismissal.append(completion)
    // UIKit must complete presentation before a forced cleanup can dismiss it.
    if !presenting { dismissHierarchy() }
  }

  private func dismissHierarchy() {
    guard !dismissing, !dismissal.isEmpty else { return }
    if let recovery {
      self.recovery = nil
      if recovery.presentingViewController != nil {
        recovery.dismiss(animated: false) { [self] in dismissHierarchy() }
        return
      }
    }
    dismissing = true
    guard let controller else { finishDismissal(); return }
    // An owner.dismiss during an interactive dismissal can be a no-op. Await
    // its existing transition first, then force again if the gesture cancels.
    if controller.isBeingDismissed, let transition = controller.transitionCoordinator {
      waitingForTransition = true
      transition.animate(alongsideTransition: nil) { [self] context in
        guard waitingForTransition else { return }
        if context.isCancelled { retryDismissal() }
        else { finishDismissal() }
      }
      return
    }
    // Dismissing through the owner removes Quick Look AND its Share/Files child;
    // dismissing the preview itself could remove only the presented child.
    if let owner = controller.presentingViewController {
      owner.dismiss(animated: false) { [self] in finishDismissal() }
    } else { finishDismissal() }
  }

  private func retryDismissal() {
    waitingForTransition = false
    // Continue after UIKit has completed the cancelled transition's callbacks.
    Task { @MainActor [weak self] in
      guard let self else { return }
      self.dismissing = false
      self.dismissHierarchy()
    }
  }

  private func finishDismissal() {
    let callbacks = dismissal
    dismissal = []
    waitingForTransition = false
    controller?.dataSource = nil
    controller?.delegate = nil
    controller?.disappeared = nil
    controller?.appeared = nil
    controller = nil
    item = nil
    ended = nil
    for callback in callbacks { callback() }
  }

  private func notifyEnded(_ error: Error?) {
    guard !didEnd else { return }
    didEnd = true
    ended?(error)
    // Lifecycle fallback if UIKit has already removed the hierarchy before its
    // transition completion callback. Share merely covering it cannot enter.
    if waitingForTransition, controller?.presentingViewController == nil { finishDismissal() }
  }

  func captureFailed(error: Error) {
    let alert = UIAlertController(
      title: NSLocalizedString("Could not save your changes.", comment: "Native PDF edit recovery title"),
      message: error.localizedDescription, preferredStyle: .alert)
    alert.addAction(UIAlertAction(title: NSLocalizedString("Continue Editing", comment: "Keep native PDF editor open"), style: .default) { _ in })
    showRecovery(alert)
  }

  func recover(error: Error, changesSaved: Bool, retry: (@MainActor () -> Void)?, discard: @escaping @MainActor () -> Void) {
    let alert = UIAlertController(
      title: changesSaved
        ? NSLocalizedString("Your changes were saved.", comment: "Native PDF edit saved before temporary cleanup failure")
        : NSLocalizedString("Could not save your changes.", comment: "Native PDF edit recovery title"),
      message: error.localizedDescription, preferredStyle: .alert)
    let id = UUID()
    if let retry {
      alert.addAction(UIAlertAction(title: NSLocalizedString("Try Again", comment: "Retry native PDF edit save"), style: .default) { [weak self, weak alert] _ in
        self?.completeRecovery(alert, id: id, action: retry)
      })
    }
    let discardTitle = changesSaved
      ? NSLocalizedString("Close", comment: "Close native PDF cleanup recovery")
      : NSLocalizedString("Discard Changes", comment: "Discard unsaved native PDF edits")
    alert.addAction(UIAlertAction(title: discardTitle, style: changesSaved ? .cancel : .destructive) { [weak self, weak alert] _ in
      self?.completeRecovery(alert, id: id, action: discard)
    })
    showRecovery(alert, id: id)
  }

  private func completeRecovery(_ alert: UIAlertController?, id: UUID, action: @escaping @MainActor () -> Void) {
    guard id == recoveryID, let alert, recovery === alert else { return }
    // Recovery may fail again immediately. Wait for real native alert dismissal
    // before the owner retries; never stack the next alert during this transition.
    alert.dismiss(animated: false) { [weak self] in
      guard let self, id == self.recoveryID else { return }
      self.recovery = nil
      action()
    }
  }

  private func showRecovery(_ alert: UIAlertController, id: UUID = UUID()) {
    recoveryID = id
    let old = recovery
    recovery = alert
    alert.view.accessibilityIdentifier = "native-pdf-edit-recovery"
    if let old, old.presentingViewController != nil {
      old.dismiss(animated: false) { [weak self] in self?.presentRecovery(id) }
    } else { presentRecovery(id) }
  }

  private func presentRecovery(_ id: UUID) {
    guard id == recoveryID, let alert = recovery, alert.presentingViewController == nil,
          var top = window?.rootViewController ?? presenter else { return }
    while let next = top.presentedViewController { top = next }
    if let transition = top.transitionCoordinator {
      transition.animate(alongsideTransition: nil) { [weak self] _ in
        // The completion still runs inside UIKit's transition callbacks.
        Task { @MainActor [weak self] in self?.presentRecovery(id) }
      }
      return
    }
    guard top.viewIfLoaded?.window != nil, !top.isBeingPresented, !top.isBeingDismissed else { return }
    top.present(alert, animated: true)
  }

  func numberOfPreviewItems(in controller: QLPreviewController) -> Int { item == nil ? 0 : 1 }
  func previewController(_ controller: QLPreviewController, previewItemAt index: Int) -> any QLPreviewItem {
    item!
  }
  func previewControllerDidDismiss(_ controller: QLPreviewController) { notifyEnded(nil) }
  func previewController(_ controller: QLPreviewController, editingModeFor previewItem: any QLPreviewItem) -> QLPreviewItemEditingMode {
    editing == nil ? .disabled : .createCopy
  }
  func previewController(_ controller: QLPreviewController, didSaveEditedCopyOf previewItem: any QLPreviewItem, at modifiedContentsURL: URL) {
    editing?.receive(modifiedContentsURL)
  }
}
