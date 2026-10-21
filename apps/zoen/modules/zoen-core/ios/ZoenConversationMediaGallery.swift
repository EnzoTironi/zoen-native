import UIKit

// Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838:
// GalleryController:1668–1737 (selected canonical item, item-specific chrome),
// GalleryPagerNode:420–424,680–716 (20pt gap and current±1 display window),
// GalleryControllerNode:560–610 (vertical dismiss). GPLv3 selected.
// UIKit adapts the existing port; ZoenFilePreview retains every disposable copy.
@MainActor final class ZoenConversationMediaGallery: NSObject, ZoenFilePreviewPresentation {
  struct Item: Sendable {
    let message: ZoenConversationGalleryRequest.Message
    let name: String
    let mime: String
    let sha256: String
  }
  private weak var presenter: UIViewController?
  private let items: [Item]
  private let selectedId: String
  private let copies: ZoenFilePreview.ReadOnlyCopies
  private let load: @Sendable (Item) async throws -> UUID
  private var controller: Controller?
  private var ended: (@MainActor (Error?) -> Void)?
  private var completion: [@MainActor () -> Void] = []
  private var presenting = false
  private var dismissing = false
  private var waiting = false
  private var didEnd = false

  init(presenter: UIViewController?, items: [Item], selectedId: String,
       copies: ZoenFilePreview.ReadOnlyCopies,
       load: @escaping @Sendable (Item) async throws -> UUID) {
    self.presenter = presenter; self.items = items; self.selectedId = selectedId
    self.copies = copies; self.load = load
  }

  func show(file: URL, title: String, ended: @escaping @MainActor (Error?) -> Void) throws {
    guard controller == nil, let presenter, presenter.viewIfLoaded?.window != nil,
          presenter.presentedViewController == nil, presenter.transitionCoordinator == nil,
          !presenter.isBeingPresented, !presenter.isBeingDismissed else { throw ZoenFilePreview.Failure.unavailable }
    let controller = Controller(items: items, selectedId: selectedId, initial: file, copies: copies, load: load)
    self.controller = controller; self.ended = ended
    controller.closed = { [weak self] error in
      self?.dismiss { [weak self] in self?.notifyEnded(error) }
    }
    controller.disappeared = { [weak self] in
      guard let self, !presenting, !dismissing else { return }
      notifyEnded(nil)
    }
    presenting = true
    presenter.present(controller, animated: false) { [self] in
      presenting = false
      if controller.presentingViewController == nil { notifyEnded(ZoenFilePreview.Failure.unavailable) }
      else {controller.animateIn()}
      drainDismissal()
    }
  }

  func dismiss(completion: @escaping @MainActor () -> Void) {
    self.completion.append(completion)
    controller?.stop()
    drainDismissal()
  }
  private func drainDismissal() {
    guard !presenting, !dismissing, !waiting, !completion.isEmpty else { return }
    guard let controller else { finishDismissal(); return }
    if let transition = controller.transitionCoordinator ?? controller.presentedViewController?.transitionCoordinator {
      waiting = true
      if transition.animate(alongsideTransition: nil, completion: { [weak self] _ in
        Task { @MainActor [weak self] in self?.waiting = false; self?.drainDismissal() }
      }) { return }
      waiting = false
    }
    dismissing = true
    if let owner = controller.presentingViewController { owner.dismiss(animated: false) { [self] in finishDismissal() } }
    else { finishDismissal() }
  }
  private func finishDismissal() {
    controller?.stop(); controller = nil; dismissing = false
    let callbacks = completion; completion.removeAll()
    for callback in callbacks { callback() }
  }
  private func notifyEnded(_ error: Error?) {
    guard !didEnd else { return }; didEnd = true
    controller?.stop(); let callback = ended; ended = nil; callback?(error)
  }

  @MainActor private final class Controller: UIViewController {
    private let items: [Item]
    private var selectedId: String
    private let copies: ZoenFilePreview.ReadOnlyCopies
    private let load: @Sendable (Item) async throws -> UUID
    private var uris: [String: URL]
    private var loader: Task<Void, Never>?
    private var revision = UUID().uuidString
    private var retired = false
    private var failure: String?
    private lazy var gallery = ZoenAttachmentGalleryContent { [weak self] action, payload in self?.action(action, payload: payload) }
    var closed: ((Error?) -> Void)?
    var disappeared: (() -> Void)?
    init(items: [Item], selectedId: String, initial: URL, copies: ZoenFilePreview.ReadOnlyCopies,
         load: @escaping @Sendable (Item) async throws -> UUID) {
      self.items = items; self.selectedId = selectedId; self.copies = copies; self.load = load
      uris = [selectedId: initial]
      super.init(nibName: nil, bundle: nil)
      modalPresentationStyle = .overFullScreen
      modalPresentationCapturesStatusBarAppearance = true
    }
    required init?(coder: NSCoder) { nil }
    override var prefersStatusBarHidden: Bool { true }
    override var supportedInterfaceOrientations: UIInterfaceOrientationMask { .allButUpsideDown }
    override func viewDidLoad() {
      super.viewDidLoad(); view.backgroundColor = .clear
      view.addSubview(gallery.view)
      refresh(); loadWindow()
    }
    override func viewDidLayoutSubviews() {
      super.viewDidLayoutSubviews(); gallery.view.frame = view.bounds
      gallery.updatePresentation(keyboardHeight: 0, safeArea: view.safeAreaInsets)
    }
    override func viewDidDisappear(_ animated: Bool) {
      super.viewDidDisappear(animated)
      if isBeingDismissed || presentingViewController == nil { stop(); disappeared?() }
    }
    private func refresh() {
      guard !retired, let index = items.firstIndex(where: { $0.message.reference.messageId == selectedId }) else { return }
      let model: [String: Any] = [
        "source": "conversation", "screen": "preview", "revision": revision,
        "selected": [String](), "previewId": selectedId,
        "items": items.map { item -> [String: Any] in
          var value: [String: Any] = ["id": item.message.reference.messageId, "name": item.name, "mime": item.mime]
          if let url = uris[item.message.reference.messageId] { value["uri"] = url.absoluteString }
          return value
        },
        "caption": ["value": items[index].message.caption, "editable": false, "sendEnabled": false],
        "recipient": items[index].message.author,
        "position": "\(index + 1) / \(items.count)",
        "labels": ["close": NSLocalizedString("Close", comment: "Close conversation media"), "retry": NSLocalizedString("Try again", comment: "Retry conversation media")],
        "errorText": failure ?? ""
      ]
      do {
        try gallery.update(model: model, appearance: .init(background: .black, text: .white,
          secondary: .lightGray, accent: .white, separator: .darkGray, bottomInset: 0))
      } catch { stop(); closed?(error) }
    }
    private func action(_ action: String, payload: [String: Any]) {
      guard !retired, payload["revision"] as? String == revision else { return }
      if action == "back" {
        stop(); gallery.animateConversationOut { [weak self] in self?.closed?(nil) }
      }
      else if action == "preview", let id = payload["id"] as? String,
              items.contains(where: { $0.message.reference.messageId == id }) {
        selectedId = id; revision = UUID().uuidString; failure = nil; refresh(); loadWindow()
      } else if action == "retry" { failure = nil; refresh(); loadWindow() }
      // No Send, edit, selection, add, reorder or implicit export in this host.
    }
    private func loadWindow() {
      loader?.cancel()
      guard let index = items.firstIndex(where: { $0.message.reference.messageId == selectedId }) else { return }
      let indices = [index, index - 1, index + 1].filter { items.indices.contains($0) }
      let wanted = Set(indices.map { items[$0].message.reference.messageId })
      uris = uris.filter { wanted.contains($0.key) }
      // Retire display requests/players before FilePreview removes old bytes.
      refresh()
      do { try copies.release(except: wanted) }
      catch { stop(); closed?(error); return }
      let focused = selectedId
      loader = Task { @MainActor [weak self] in
        guard let self else { return }
        for index in indices {
          let item = items[index], id = item.message.reference.messageId
          guard !retired, !Task.isCancelled, selectedId == focused else { return }
          if uris[id] != nil { continue }
          do {
            let ticket = try await load(item)
            // A late copy still transfers to the same FilePreview owner, which
            // deletes it even if this page or its entire viewer already retired.
            let url = try copies.accept(ticket, key: id)
            guard !retired, !Task.isCancelled, selectedId == focused else {
              try copies.release(except: Set(uris.keys)); return
            }
            uris[id] = url; refresh()
          } catch {
            if Task.isCancelled || retired { return }
            if case ZoenFilePreview.Failure.changed = error { stop(); closed?(error); return }
            if id == selectedId { failure = NSLocalizedString("This file could not be opened. Try again.", comment: "Conversation gallery page failed"); refresh() }
          }
        }
      }
    }
    func stop() {
      guard !retired else { return }; retired = true
      loader?.cancel(); loader = nil; gallery.retire(); uris.removeAll()
    }
    func animateIn() {gallery.animateConversationIn()}
  }
}
