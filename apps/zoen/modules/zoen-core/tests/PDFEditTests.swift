import Foundation

@MainActor private final class EditSignal {
  private var ready = false
  private var waiters: [CheckedContinuation<Void, Never>] = []
  func emit() { ready = true; waiters.forEach { $0.resume() }; waiters.removeAll() }
  func wait() async { if !ready { await withCheckedContinuation { waiters.append($0) } } }
}

@MainActor private final class EditPresentation: ZoenFilePreviewPresentation {
  let shown = EditSignal()
  let dismissRequested = EditSignal()
  let recoveryRequested = EditSignal()
  var ended: ((Error?) -> Void)?
  var dismissed: (() -> Void)?
  var retry: (() -> Void)?
  var discard: (() -> Void)?
  var capturesFailed = 0
  var changesSaved = false
  var dismissCount = 0
  func show(file: URL, title: String, ended: @escaping @MainActor (Error?) -> Void) throws {
    self.ended = ended; shown.emit()
  }
  func dismiss(completion: @escaping @MainActor () -> Void) {
    dismissCount += 1; dismissed = completion; dismissRequested.emit()
  }
  func recover(error: Error, changesSaved: Bool, retry: (@MainActor () -> Void)?, discard: @escaping @MainActor () -> Void) {
    self.changesSaved = changesSaved
    self.retry = retry; self.discard = discard; recoveryRequested.emit()
  }
  func captureFailed(error: Error) { capturesFailed += 1 }
}

@MainActor private final class EditResult {
  var result: Result<Void, Error>?
  var editing: ZoenFilePreview.Editing?
  func run(_ action: () async throws -> Void) async {
    do { try await action(); result = .success(()) } catch { result = .failure(error) }
  }
  func success() { guard case .success? = result else { preconditionFailure("Expected successful completion: \(String(describing: result))") } }
}

private actor EditStore {
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

@MainActor private final class EditFixture {
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

@main @MainActor struct PDFEditTests {
  private static let base = ZoenFilePreview.EditBase(version: 7, sha256: "canonical-seven")
  private static func begin(_ owner: ZoenFilePreview, file: ZoenPreviewFile, store: EditStore,
                            presentation: EditPresentation, result: EditResult) -> Task<Void, Never> {
    let ticket = owner.register(file: file, generation: 1)
    let context = ZoenFilePreview.EditContext(base: base, maximumBytes: 4096) { file, base in try await store.save(file, base) }
    return Task { @MainActor in
      await result.run {
        try await owner.open(id: ticket, title: "Synthetic edited PDF", edit: context) { editing in
          result.editing = editing
          return presentation
        }
      }
    }
  }

  static func main() async throws {
    try await serialSavesOwnTheirCopiesAndAdvanceBase()
    try await failedSaveRetainsCopyUntilRetry()
    try await discardIsExplicitAndDoesNotRetry()
    try await forcedCleanupWaitsWorkerAndDoesNotSaveQueuedCopy()
    try await rowCancellationDoesNotDiscardMarkup()
    try await acceptedSaveCleanupRetryDoesNotSaveAgain()
    try await captureFailureRequiresAcknowledgedDiscard()
    try captureRejectsUnsafeOrUnboundedSources()
    print("Synthetic PDF edit tests passed: 8 scenarios; synchronous capture, serialized expected bases, row-independent retry/discard, graceful close, forced worker drain, cancellation, accepted-save cleanup retry and bounded regular PDF staging.")
  }

  private static func serialSavesOwnTheirCopiesAndAdvanceBase() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(held: true), presentation = EditPresentation(), result = EditResult()
    let first = try fixture.file("edited-one"), second = try fixture.file("edited-two", content: "%PDF-1.7\nSecond complete edit\n%%EOF")
    let task = begin(owner, file: try fixture.file("original"), store: store, presentation: presentation, result: result)
    await presentation.shown.wait()
    result.editing!.receive(first.url); result.editing!.receive(second.url)
    try first.remove(); try second.remove()
    await store.waitForCalls(1)
    presentation.ended?(nil)
    let closing = EditResult()
    let close = Task { @MainActor in await closing.run { try await owner.prepareForClose() } }
    await Task.yield()
    precondition(result.result == nil && closing.result == nil, "Dismissal/close resolved before saves settled")
    await store.release(); await task.value; await close.value
    result.success(); closing.success()
    let bases = await store.bases, files = await store.files
    precondition(bases == [base, .init(version: 8, sha256: "accepted-8")])
    precondition(files.count == 2 && files.allSatisfy { !FileManager.default.fileExists(atPath: $0.directory.path) })
  }

  private static func failedSaveRetainsCopyUntilRetry() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(failures: 1), presentation = EditPresentation(), result = EditResult()
    let task = begin(owner, file: try fixture.file("original"), store: store, presentation: presentation, result: result)
    await presentation.shown.wait(); result.editing!.receive(try fixture.file("edited").url); presentation.ended?(nil)
    await presentation.recoveryRequested.wait()
    let files = await store.files
    precondition(!presentation.changesSaved)
    precondition(result.result == nil && FileManager.default.fileExists(atPath: files[0].url.path))
    precondition(presentation.retry != nil)
    presentation.retry?(); await task.value; result.success()
    let retriedBases = await store.bases
    precondition(retriedBases == [base, base], "Retry changed the captured expected version")
    precondition(!FileManager.default.fileExists(atPath: files[0].directory.path))
  }

  private static func discardIsExplicitAndDoesNotRetry() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(failures: 1), presentation = EditPresentation(), result = EditResult()
    let task = begin(owner, file: try fixture.file("original"), store: store, presentation: presentation, result: result)
    await presentation.shown.wait(); result.editing!.receive(try fixture.file("edited").url); presentation.ended?(nil)
    await presentation.recoveryRequested.wait(); precondition(result.result == nil)
    presentation.discard?(); await task.value; result.success()
    let callCount = await store.bases.count
    precondition(callCount == 1)
    let files = await store.files
    precondition(!FileManager.default.fileExists(atPath: files[0].directory.path))
  }

  private static func forcedCleanupWaitsWorkerAndDoesNotSaveQueuedCopy() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(held: true), presentation = EditPresentation(), result = EditResult()
    let task = begin(owner, file: try fixture.file("original"), store: store, presentation: presentation, result: result)
    await presentation.shown.wait()
    result.editing!.receive(try fixture.file("edited-one").url); result.editing!.receive(try fixture.file("edited-two").url)
    await store.waitForCalls(1)
    let clearing = EditResult()
    let clear = Task { @MainActor in await clearing.run { try await owner.clear(before: 2) } }
    await presentation.dismissRequested.wait(); presentation.dismissed?(); await Task.yield()
    precondition(result.result == nil && clearing.result == nil, "Force cleanup finished while save worker could still write")
    await store.release(); await clear.value; await task.value; clearing.success()
    guard case .failure? = result.result else { preconditionFailure("Forced preview must retire") }
    let callCount = await store.bases.count
    precondition(callCount == 1, "Forced cleanup started another queued write")
    let files = await store.files
    precondition(!FileManager.default.fileExists(atPath: files[0].directory.path))
  }

  private static func rowCancellationDoesNotDiscardMarkup() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(), presentation = EditPresentation(), result = EditResult()
    let task = begin(owner, file: try fixture.file("original"), store: store, presentation: presentation, result: result)
    await presentation.shown.wait(); task.cancel(); await Task.yield(); await Task.yield()
    precondition(presentation.dismissCount == 0 && result.result == nil)
    presentation.ended?(nil); await task.value; result.success()
  }

  private static func acceptedSaveCleanupRetryDoesNotSaveAgain() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(denyDeletion: true), presentation = EditPresentation(), result = EditResult()
    let task = begin(owner, file: try fixture.file("original"), store: store, presentation: presentation, result: result)
    await presentation.shown.wait(); result.editing!.receive(try fixture.file("edited").url); presentation.ended?(nil)
    await presentation.recoveryRequested.wait()
    precondition(presentation.changesSaved, "Cleanup-only recovery must not offer to discard an already-saved version")
    let files = await store.files
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: files[0].directory.path)
    presentation.retry?(); await task.value; result.success()
    let callCount = await store.bases.count
    precondition(callCount == 1, "A cleanup retry created another canonical version")
    precondition(!FileManager.default.fileExists(atPath: files[0].directory.path))
  }

  private static func captureFailureRequiresAcknowledgedDiscard() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(), presentation = EditPresentation(), result = EditResult()
    let task = begin(owner, file: try fixture.file("original"), store: store, presentation: presentation, result: result)
    await presentation.shown.wait(); result.editing!.receive(try fixture.file("invalid", content: "not a PDF").url)
    precondition(presentation.capturesFailed == 1)
    presentation.ended?(nil); await presentation.recoveryRequested.wait()
    precondition(result.result == nil && presentation.retry == nil)
    presentation.discard?(); await task.value; result.success()
    let callCount = await store.bases.count
    precondition(callCount == 0)
  }

  private static func captureRejectsUnsafeOrUnboundedSources() throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let good = try fixture.file("source")
    let link = fixture.root.appendingPathComponent("substituted.pdf")
    try FileManager.default.createSymbolicLink(at: link, withDestinationURL: good.url)
    for (url, limit) in [(link, UInt64(4096)), (good.url, 4), (try fixture.file("empty", content: "").url, 4096)] {
      guard case .failure(let error) = ZoenPDFEditCapture.stage(url, maximumBytes: limit) else { preconditionFailure("Invalid edited-copy source admitted") }
      precondition(error.retained == nil)
    }
    precondition(FileManager.default.fileExists(atPath: good.url.path), "Capture modified the framework-owned original")
  }
}
