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
  private var controller: Controller?
  private var item: Item?
  private var ended: (@MainActor (Error?) -> Void)?
  private var didEnd = false
  private var presenting = false
  private var dismissing = false
  private var waitingForTransition = false
  private var dismissal: [@MainActor () -> Void] = []

  init(presenter: UIViewController?) { self.presenter = presenter }

  func show(file: URL, title: String, ended: @escaping @MainActor (Error?) -> Void) throws {
    guard controller == nil, let presenter,
          presenter.viewIfLoaded?.window != nil,
          !presenter.isBeingPresented, !presenter.isBeingDismissed,
          presenter.transitionCoordinator == nil, presenter.presentedViewController == nil,
          QLPreviewController.canPreview(file as NSURL) else { throw ZoenFilePreview.Failure.unavailable }
    let controller = Controller()
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
    dismissal.append(completion)
    // UIKit must complete presentation before a forced cleanup can dismiss it.
    if !presenting { dismissHierarchy() }
  }

  private func dismissHierarchy() {
    guard !dismissing, !dismissal.isEmpty else { return }
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

  func numberOfPreviewItems(in controller: QLPreviewController) -> Int { item == nil ? 0 : 1 }
  func previewController(_ controller: QLPreviewController, previewItemAt index: Int) -> any QLPreviewItem {
    item!
  }
  func previewControllerDidDismiss(_ controller: QLPreviewController) { notifyEnded(nil) }
  func previewController(_ controller: QLPreviewController, editingModeFor previewItem: any QLPreviewItem) -> QLPreviewItemEditingMode {
    .disabled
  }
}
