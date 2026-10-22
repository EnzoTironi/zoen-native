import Foundation

@MainActor final class PreviewPresentation: ZoenFilePreviewPresentation {
  enum PresentationFailure: Error { case unavailable }

  var failShow = false
  private(set) var shownFile: URL?
  private(set) var shownTitle: String?
  private(set) var dismissCount = 0
  private var ended: (@MainActor (Error?) -> Void)?
  private var dismissals: [@MainActor () -> Void] = []
  private var showWaiters: [CheckedContinuation<Void, Never>] = []
  private var dismissWaiters: [CheckedContinuation<Void, Never>] = []

  func show(file: URL, title: String, ended: @escaping @MainActor (Error?) -> Void) throws {
    shownFile = file
    shownTitle = title
    self.ended = ended
    for waiter in showWaiters { waiter.resume() }
    showWaiters.removeAll()
    if failShow { throw PresentationFailure.unavailable }
  }

  func dismiss(completion: @escaping @MainActor () -> Void) {
    dismissCount += 1
    dismissals.append(completion)
    for waiter in dismissWaiters { waiter.resume() }
    dismissWaiters.removeAll()
  }

  func waitUntilShown() async {
    if shownFile != nil { return }
    await withCheckedContinuation { showWaiters.append($0) }
  }

  func waitUntilDismissRequested() async {
    if dismissCount > 0 { return }
    await withCheckedContinuation { dismissWaiters.append($0) }
  }

  func endNormally(error: Error? = nil) { ended?(error) }

  func completeHierarchyDismissal() {
    precondition(!dismissals.isEmpty, "No pending native hierarchy dismissal")
    dismissals.removeFirst()()
  }
}

@MainActor final class PreviewCompletion {
  private(set) var result: Result<Void, Error>?
  var onCompletion: (() -> Void)?
  var finished: Bool { result != nil }

  func run(_ operation: @MainActor () async throws -> Void) async {
    do {
      try await operation()
      onCompletion?()
      result = .success(())
    } catch {
      onCompletion?()
      result = .failure(error)
    }
  }

  func expectSuccess() {
    guard case .success? = result else { preconditionFailure("Expected completed success: \(String(describing: result))") }
  }

  func expectFailure(_ matches: (Error) -> Bool) {
    guard case .failure(let error)? = result else { preconditionFailure("Expected completed failure") }
    precondition(matches(error), "Unexpected failure: \(error)")
  }
}

@MainActor final class PreviewSignal {
  private var emitted = false
  private var waiters: [CheckedContinuation<Void, Never>] = []
  func emit() {
    emitted = true
    for waiter in waiters { waiter.resume() }
    waiters.removeAll()
  }
  func wait() async {
    if emitted { return }
    await withCheckedContinuation { waiters.append($0) }
  }
}

@MainActor final class PreviewFixture {
  let root: URL
  private let manager = FileManager.default

  init() throws {
    root = manager.temporaryDirectory.appendingPathComponent("zoen-preview-fixture-\(UUID().uuidString)", isDirectory: true)
    try manager.createDirectory(at: root, withIntermediateDirectories: true)
  }

  func file(_ name: String) throws -> ZoenPreviewFile {
    let directory = root.appendingPathComponent(name, isDirectory: true)
    try manager.createDirectory(at: directory, withIntermediateDirectories: true)
    let url = directory.appendingPathComponent("synthetic.pdf")
    try Data("Synthetic preview lease only".utf8).write(to: url)
    return ZoenPreviewFile(url: url, directory: directory)
  }

  func exists(_ file: ZoenPreviewFile) -> Bool { manager.fileExists(atPath: file.url.path) }
  func cleanup() { try? manager.removeItem(at: root) }
}
