import Foundation

@MainActor final class EditSignal {
  private var ready = false
  private var waiters: [CheckedContinuation<Void, Never>] = []
  func emit() { ready = true; waiters.forEach { $0.resume() }; waiters.removeAll() }
  func wait() async { if !ready { await withCheckedContinuation { waiters.append($0) } } }
}

@MainActor final class EditPresentation: ZoenFilePreviewPresentation {
  let shown = EditSignal()
  let dismissRequested = EditSignal()
  let recoveryRequested = EditSignal()
  var ended: ((Error?) -> Void)?
  var dismissed: (() -> Void)?
  var retry: (() -> Void)?
  var keep: (() -> Void)?
  var discard: (() -> Void)?
  var capturesFailed = 0
  var changesSaved = false
  var recoveryCount = 0
  var onRecovery: (() -> Void)?
  var onShown: (() -> Void)?
  var dismissCount = 0
  var shownFile: URL?
  var recoveryError: Error?
  func show(file: URL, title: String, ended: @escaping @MainActor (Error?) -> Void) throws {
    self.shownFile = file; self.ended = ended; shown.emit(); onShown?()
  }
  func dismiss(completion: @escaping @MainActor () -> Void) {
    dismissCount += 1; dismissed = completion; dismissRequested.emit()
  }
  func recover(error: Error, changesSaved: Bool, retry: (@MainActor () -> Void)?, discard: @escaping @MainActor () -> Void) {
    recover(error: error, changesSaved: changesSaved, retry: retry, keep: nil, discard: discard)
  }
  func recover(error: Error, changesSaved: Bool, retry: (@MainActor () -> Void)?, keep: (@MainActor () -> Void)?, discard: @escaping @MainActor () -> Void) {
    self.changesSaved = changesSaved
    recoveryError = error
    recoveryCount += 1
    self.retry = retry; self.keep = keep; self.discard = discard; recoveryRequested.emit()
    onRecovery?()
  }
  func captureFailed(error: Error) { capturesFailed += 1 }
}

final class EditRetention: @unchecked Sendable {
  enum Failure: Error { case unavailable }
  private let lock = NSLock()
  private var failures: Int
  private var attempts: [ZoenPreviewFile] = []
  private var accepted: Set<String> = []
  init(failures: Int = 0) { self.failures = failures }
  func retain(_ file: ZoenPreviewFile) throws {
    lock.lock(); defer { lock.unlock() }
    precondition(FileManager.default.fileExists(atPath: file.url.path))
    attempts.append(file)
    if failures > 0 { failures -= 1; throw Failure.unavailable }
    accepted.insert(file.url.absoluteString)
  }
  func contains(_ file: ZoenPreviewFile) -> Bool {
    lock.lock(); defer { lock.unlock() }
    return accepted.contains(file.url.absoluteString)
  }
  func files() -> [ZoenPreviewFile] {
    lock.lock(); defer { lock.unlock() }
    return attempts
  }
}

actor EditDiscard {
  enum Failure: Error { case unavailable }
  var calls = 0
  private var failures: Int
  private var held: Bool
  private var releases: [CheckedContinuation<Void, Never>] = []
  private var waiters: [(Int, CheckedContinuation<Void, Never>)] = []
  init(failures: Int = 0, held: Bool = false) { self.failures = failures; self.held = held }
  func discard() async throws {
    calls += 1
    let ready = waiters.filter { $0.0 <= calls }
    waiters.removeAll { $0.0 <= calls }; ready.forEach { $0.1.resume() }
    if held { await withCheckedContinuation { releases.append($0) } }
    if failures > 0 { failures -= 1; throw Failure.unavailable }
  }
  func waitForCalls(_ count: Int) async {
    if calls < count { await withCheckedContinuation { waiters.append((count, $0)) } }
  }
  func release() { held = false; releases.forEach { $0.resume() }; releases.removeAll() }
}

@MainActor final class EditResult {
  var result: Result<Void, Error>?
  var editing: ZoenFilePreview.Editing?
  var onCompletion: (() -> Void)?
  func run(_ action: () async throws -> Void) async {
    do { try await action(); result = .success(()) } catch { result = .failure(error) }
    onCompletion?()
  }
  func success() { guard case .success? = result else { preconditionFailure("Expected successful completion: \(String(describing: result))") } }
}

actor EditStore {
  enum SaveError: Error { case unavailable }
  var bases: [ZoenFilePreview.EditBase] = []
  var files: [ZoenPreviewFile] = []
  var failures: Int
  var held: Bool
  var denyDeletion: Bool
  private var releases: [CheckedContinuation<Void, Never>] = []
  private var callWaiters: [(Int, CheckedContinuation<Void, Never>)] = []
  init(failures: Int = 0, held: Bool = false, denyDeletion: Bool = false) {
    self.failures = failures; self.held = held; self.denyDeletion = denyDeletion
  }
  func save(_ file: ZoenPreviewFile, _ base: ZoenFilePreview.EditBase) async throws -> ZoenFilePreview.EditBase {
    precondition(FileManager.default.fileExists(atPath: file.url.path), "A save lost its owned edited copy")
    bases.append(base); files.append(file)
    let ready = callWaiters.filter { $0.0 <= bases.count }
    callWaiters.removeAll { $0.0 <= bases.count }
    ready.forEach { $0.1.resume() }
    if held { await withCheckedContinuation { releases.append($0) } }
    if failures > 0 { failures -= 1; throw SaveError.unavailable }
    if denyDeletion { try FileManager.default.setAttributes([.posixPermissions: 0o000], ofItemAtPath: file.directory.path) }
    return ZoenFilePreview.EditBase(version: base.version + 1, sha256: "accepted-\(base.version + 1)")
  }
  func waitForCalls(_ count: Int) async {
    if bases.count < count { await withCheckedContinuation { callWaiters.append((count, $0)) } }
  }
  func release() { held = false; releases.forEach { $0.resume() }; releases.removeAll() }
}

@MainActor final class EditFixture {
  let root: URL
  init() throws {
    root = FileManager.default.temporaryDirectory.appendingPathComponent("zoen-pdf-edit-test-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
  }
  func file(_ name: String, content: String = "%PDF-1.7\nSynthetic QA only\n%%EOF\n") throws -> ZoenPreviewFile {
    let directory = root.appendingPathComponent(name, isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    let file = ZoenPreviewFile(url: directory.appendingPathComponent("QA.pdf"), directory: directory)
    try Data(content.utf8).write(to: file.url)
    return file
  }
  func remove() { try? FileManager.default.removeItem(at: root) }
}
