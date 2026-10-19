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

private final class EditRetention: @unchecked Sendable {
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

private actor EditDiscard {
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

@MainActor private final class EditResult {
  var result: Result<Void, Error>?
  var editing: ZoenFilePreview.Editing?
  var onCompletion: (() -> Void)?
  func run(_ action: () async throws -> Void) async {
    do { try await action(); result = .success(()) } catch { result = .failure(error) }
    onCompletion?()
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
                            presentation: EditPresentation, result: EditResult,
                            recovered: [ZoenPreviewFile] = [], context supplied: ZoenFilePreview.EditContext? = nil) -> Task<Void, Never> {
    let ticket = owner.register(file: file, generation: 1, recovered: recovered)
    let context = supplied ?? ZoenFilePreview.EditContext(base: base, maximumBytes: 4096) { file, base in try await store.save(file, base) }
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
    try await retryOlderCopyPreservesNewerCaptureFailure()
    try await closeRetiresRegisteredEditorBeforePresentation()
    try captureRejectsUnsafeOrUnboundedSources()
    try await everyCaptureRetainsSynchronouslyBeforePublicSave()
    try await recoveredCopiesWaitForExplicitSaveAndAdvanceBases()
    try await recoveredTransfersCloseBeforePresentationAndDrain()
    try await failedRetainKeepsOwnedCopyForSaveRetry()
    try await failedAsyncDiscardRetriesOnlyDiscard()
    try await forcedClearPreservesRecoveredJournal()
    try await forcedClearDrainsAdmittedDiscard()
    try await partialTransfersAbandonOnceAndYieldToClear()
    try await keepForLaterRetainsWithoutPublishingOrDiscarding()
    try await failedKeepRetainsCopiesAndRetriesOnlyKeep()
    try await recoveredReadOnlyViewerRejectsNewCaptures()
    print("Synthetic PDF edit tests passed: 21 scenarios; existing capture/serialized save/lifetime cases; synchronous private retain, explicit recovered save, recovered transfer ownership, retain retry, asynchronous discard failure/retry, forced drain without implicit journal discard, partial transfer cleanup, Keep for Later and read-only recovered editing admission.")
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
    let close = Task { @MainActor in await closing.run { try await owner.prepareForClose(before: 2) } }
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

  private static func retryOlderCopyPreservesNewerCaptureFailure() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(failures: 1, held: true)
    let presentation = EditPresentation(), result = EditResult()
    let task = begin(owner, file: try fixture.file("original"), store: store, presentation: presentation, result: result)
    await presentation.shown.wait()
    result.editing!.receive(try fixture.file("older-valid-edit").url)
    await store.waitForCalls(1)
    result.editing!.receive(try fixture.file("newer-failed-edit", content: "not a retained PDF").url)
    precondition(presentation.capturesFailed == 1)
    await store.release()
    presentation.ended?(nil)
    await presentation.recoveryRequested.wait()
    precondition(presentation.retry != nil && result.result == nil)
    let changed = EditSignal()
    presentation.onRecovery = { changed.emit() }
    result.onCompletion = { changed.emit() }
    presentation.retry?()
    await changed.wait()
    precondition(result.result == nil && presentation.recoveryCount == 2,
                 "Retrying an older save silently acknowledged a newer edit that was never captured")
    precondition(presentation.retry == nil, "No staged copy exists for the newer failed edit")
    let callCount = await store.bases.count
    precondition(callCount == 2)
    presentation.discard?()
    await task.value
    result.success()
  }

  private static func closeRetiresRegisteredEditorBeforePresentation() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(), presentation = EditPresentation()
    let source = try fixture.file("registered-editor")
    let ticket = owner.register(file: source, generation: 1)
    let context = ZoenFilePreview.EditContext(base: base, maximumBytes: 4096) { file, base in
      try await store.save(file, base)
    }
    // This is the adverse actor order: close inspects an empty active slot while
    // an earlier, already registered editable open has not entered MainActor yet.
    try await owner.prepareForClose(before: 2)
    let completed = EditSignal()
    let result = EditResult()
    result.onCompletion = { completed.emit() }
    presentation.onShown = { completed.emit() }
    let task = Task { @MainActor in
      await result.run {
        try await owner.open(id: ticket, title: "Queued synthetic PDF", edit: context) { _ in
          presentation
        }
      }
    }
    // A regressed implementation enters show rather than resolving. Listen to
    // both outcomes, so the negative control fails an assertion, not a timeout.
    await completed.wait()
    guard case .failure(let error)? = result.result,
          case ZoenFilePreview.Failure.changed = error else {
      preconditionFailure("A registered editable preview appeared after graceful close prepared")
    }
    precondition(presentation.ended == nil && presentation.dismissCount == 0,
                 "Queued editor should retire before any presentation or forced dismissal")
    let callCount = await store.bases.count
    precondition(callCount == 0)
    precondition(!FileManager.default.fileExists(atPath: source.directory.path))
    await task.value
  }

  private static func everyCaptureRetainsSynchronouslyBeforePublicSave() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(held: true), journal = EditRetention()
    let presentation = EditPresentation(), result = EditResult()
    let context = ZoenFilePreview.EditContext(base: base, maximumBytes: 4096,
      retain: { try journal.retain($0) }) { file, base in
        precondition(journal.contains(file), "Public save ran before synchronous private retain")
        return try await store.save(file, base)
      }
    let task = begin(owner, file: try fixture.file("viewer"), store: store,
      presentation: presentation, result: result, context: context)
    await presentation.shown.wait()
    result.editing!.receive(try fixture.file("first").url)
    result.editing!.receive(try fixture.file("second").url)
    precondition(journal.files().count == 2, "Delegate returned before all captures were privately retained")
    await store.waitForCalls(1)
    result.editing!.receive(try fixture.file("third").url)
    precondition(journal.files().count == 3, "A held first save blocked private retention of a later capture")
    presentation.ended?(nil); await store.release(); await task.value; result.success()
    let bases = await store.bases
    precondition(bases == [base, .init(version: 8, sha256: "accepted-8"), .init(version: 9, sha256: "accepted-9")])
    precondition(journal.files().allSatisfy { !FileManager.default.fileExists(atPath: $0.directory.path) })
  }

  private static func recoveredCopiesWaitForExplicitSaveAndAdvanceBases() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(), journal = EditRetention()
    let presentation = EditPresentation(), result = EditResult()
    let viewer = try fixture.file("independent-latest-viewer")
    let recovered = [try fixture.file("recovered-first"), try fixture.file("recovered-second")]
    let context = ZoenFilePreview.EditContext(base: base, maximumBytes: 4096,
      retain: { try journal.retain($0) }) { file, base in try await store.save(file, base) }
    let task = begin(owner, file: viewer, store: store, presentation: presentation,
      result: result, recovered: recovered, context: context)
    await presentation.shown.wait()
    precondition(presentation.shownFile == viewer.url)
    result.editing!.receive(try fixture.file("newer-private-edit").url)
    precondition(journal.files().count == 1, "Recovered state prevented retention of a new private capture")
    let changed = EditSignal()
    presentation.onRecovery = { changed.emit() }; result.onCompletion = { changed.emit() }
    presentation.ended?(nil); await changed.wait()
    let before = await store.bases.count
    precondition(before == 0 && result.result == nil && presentation.retry != nil,
      "Recovered edits or a newer private capture published without explicit Save")
    guard case .recovered? = presentation.recoveryError as? ZoenFilePreview.Failure else {
      preconditionFailure("Recovered changes lost their explicit recovery explanation")
    }
    let obsoleteDiscard = presentation.discard
    presentation.retry?()
    obsoleteDiscard?()
    result.editing!.receive(try fixture.file("last-capture-during-save").url)
    precondition(journal.files().count == 2, "An obsolete recovery action started discard behind Save")
    await task.value; result.success()
    let bases = await store.bases
    precondition(bases == [base, .init(version: 8, sha256: "accepted-8"), .init(version: 9, sha256: "accepted-9"), .init(version: 10, sha256: "accepted-10")])
    precondition(([viewer] + recovered + journal.files()).allSatisfy { !FileManager.default.fileExists(atPath: $0.directory.path) })
  }

  private static func recoveredTransfersCloseBeforePresentationAndDrain() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(), presentation = EditPresentation()
    let viewer = try fixture.file("queued-viewer"), recovered = [try fixture.file("queued-copy-one"), try fixture.file("queued-copy-two")]
    let ticket = owner.register(file: viewer, generation: 1, recovered: recovered)
    let context = ZoenFilePreview.EditContext(base: base, maximumBytes: 4096) { file, base in try await store.save(file, base) }
    try await owner.prepareForClose(before: 2)
    do {
      try await owner.open(id: ticket, title: "Retired recovered PDF", edit: context) { _ in presentation }
      preconditionFailure("Retired recovered preview opened")
    } catch ZoenFilePreview.Failure.changed {}
    precondition(presentation.shownFile == nil)
    precondition(([viewer] + recovered).allSatisfy { !FileManager.default.fileExists(atPath: $0.directory.path) })
    let next = try fixture.file("unopened-viewer"), copies = [try fixture.file("unopened-copy")]
    _ = owner.register(file: next, generation: 3, recovered: copies)
    try await owner.clear(before: 4)
    precondition(([next] + copies).allSatisfy { !FileManager.default.fileExists(atPath: $0.directory.path) })
    let count = await store.bases.count
    precondition(count == 0)
  }

  private static func failedRetainKeepsOwnedCopyForSaveRetry() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(), journal = EditRetention(failures: 1)
    let presentation = EditPresentation(), result = EditResult()
    let context = ZoenFilePreview.EditContext(base: base, maximumBytes: 4096,
      retain: { try journal.retain($0) }) { file, base in
        if !journal.contains(file) { try journal.retain(file) }
        return try await store.save(file, base)
      }
    let task = begin(owner, file: try fixture.file("viewer"), store: store,
      presentation: presentation, result: result, context: context)
    await presentation.shown.wait()
    let framework = try fixture.file("framework-copy")
    result.editing!.receive(framework.url); try framework.remove()
    let owned = journal.files()[0]
    presentation.ended?(nil); await presentation.recoveryRequested.wait()
    let before = await store.bases.count
    precondition(before == 0 && result.result == nil && FileManager.default.fileExists(atPath: owned.url.path))
    presentation.retry?(); await task.value; result.success()
    let saved = await store.files
    precondition(saved.count == 1 && saved[0].url == owned.url && journal.contains(owned))
    precondition(!FileManager.default.fileExists(atPath: owned.directory.path))
  }

  private static func failedAsyncDiscardRetriesOnlyDiscard() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(), discarder = EditDiscard(failures: 1, held: true), journal = EditRetention()
    let presentation = EditPresentation(), result = EditResult(), recovered = [try fixture.file("private-copy")]
    let context = ZoenFilePreview.EditContext(base: base, maximumBytes: 4096,
      retain: { try journal.retain($0) }, discard: { try await discarder.discard() }) { file, base in try await store.save(file, base) }
    let task = begin(owner, file: try fixture.file("viewer"), store: store,
      presentation: presentation, result: result, recovered: recovered, context: context)
    await presentation.shown.wait(); presentation.ended?(nil); await presentation.recoveryRequested.wait()
    presentation.discard?(); await discarder.waitForCalls(1)
    precondition(result.result == nil && FileManager.default.fileExists(atPath: recovered[0].url.path),
      "Plaintext was deleted before the private discard ACK")
    result.editing!.receive(try fixture.file("late-after-discard").url)
    precondition(journal.files().isEmpty, "A late capture persisted behind explicit discard")
    let retryShown = EditSignal(); presentation.onRecovery = { retryShown.emit() }
    await discarder.release(); await retryShown.wait()
    precondition(result.result == nil && FileManager.default.fileExists(atPath: recovered[0].url.path))
    precondition(presentation.retry != nil)
    presentation.retry?(); await task.value; result.success()
    let discards = await discarder.calls, saves = await store.bases.count
    precondition(discards == 2 && saves == 0, "Discard retry published a recovered edit")
    precondition(!FileManager.default.fileExists(atPath: recovered[0].directory.path))
  }

  private static func forcedClearPreservesRecoveredJournal() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(), discarder = EditDiscard()
    let presentation = EditPresentation(), result = EditResult(), viewer = try fixture.file("viewer")
    let recovered = [try fixture.file("private-copy")]
    let context = ZoenFilePreview.EditContext(base: base, maximumBytes: 4096,
      discard: { try await discarder.discard() }) { file, base in try await store.save(file, base) }
    let task = begin(owner, file: viewer, store: store, presentation: presentation,
      result: result, recovered: recovered, context: context)
    await presentation.shown.wait()
    let clearing = Task { @MainActor in try await owner.clear(before: 2) }
    await presentation.dismissRequested.wait(); presentation.dismissed?()
    try await clearing.value; await task.value
    guard case .failure? = result.result else { preconditionFailure("Forced recovered preview did not retire") }
    let discards = await discarder.calls, saves = await store.bases.count
    precondition(discards == 0 && saves == 0, "Forced cleanup discarded or published the private journal")
    precondition(([viewer] + recovered).allSatisfy { !FileManager.default.fileExists(atPath: $0.directory.path) })
  }

  private static func forcedClearDrainsAdmittedDiscard() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(), discarder = EditDiscard(held: true)
    let presentation = EditPresentation(), result = EditResult(), recovered = [try fixture.file("private-copy")]
    let context = ZoenFilePreview.EditContext(base: base, maximumBytes: 4096,
      discard: { try await discarder.discard() }) { file, base in try await store.save(file, base) }
    let task = begin(owner, file: try fixture.file("viewer"), store: store,
      presentation: presentation, result: result, recovered: recovered, context: context)
    await presentation.shown.wait(); presentation.ended?(nil); await presentation.recoveryRequested.wait()
    presentation.discard?(); await discarder.waitForCalls(1)
    let cleared = EditResult()
    let clearing = Task { @MainActor in await cleared.run { try await owner.clear(before: 2) } }
    await presentation.dismissRequested.wait(); presentation.dismissed?()
    precondition(cleared.result == nil && result.result == nil,
      "Forced cleanup returned while an admitted private discard could still run")
    await discarder.release(); await clearing.value; await task.value; cleared.success()
    let discards = await discarder.calls, saves = await store.bases.count
    precondition(discards == 1 && saves == 0)
    guard case .failure? = result.result else { preconditionFailure("Forced discard preview did not retire") }
    precondition(!FileManager.default.fileExists(atPath: recovered[0].directory.path))
  }

  private static func partialTransfersAbandonOnceAndYieldToClear() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview()
    let partial = try fixture.file("partial-viewer"), copied = try fixture.file("partial-recovered")
    let id = owner.register(file: partial, generation: 1, recovered: [copied])
    try owner.abandon(id: id); try owner.abandon(id: id)
    precondition([partial, copied].allSatisfy { !FileManager.default.fileExists(atPath: $0.directory.path) })
    let next = try fixture.file("cleared-viewer"), nextCopy = try fixture.file("cleared-recovered")
    let cleared = owner.register(file: next, generation: 3, recovered: [nextCopy])
    try await owner.clear(before: 4)
    precondition([next, nextCopy].allSatisfy { !FileManager.default.fileExists(atPath: $0.directory.path) })
    // An obsolete abandon must not delete a newer file reusing the old path.
    let later = try fixture.file("cleared-viewer", content: "%PDF-1.7\nNew independent fixture\n%%EOF")
    try owner.abandon(id: cleared)
    precondition(FileManager.default.fileExists(atPath: later.url.path))
  }

  private static func keepForLaterRetainsWithoutPublishingOrDiscarding() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(), journal = EditRetention(), discarder = EditDiscard()
    let presentation = EditPresentation(), result = EditResult(), viewer = try fixture.file("viewer")
    let recovered = [try fixture.file("first-private-copy"), try fixture.file("latest-private-copy")]
    let context = ZoenFilePreview.EditContext(base: base, maximumBytes: 4096,
      retain: { try journal.retain($0) }, discard: { try await discarder.discard() }) { file, base in
        try await store.save(file, base)
      }
    let task = begin(owner, file: viewer, store: store, presentation: presentation,
      result: result, recovered: recovered, context: context)
    await presentation.shown.wait(); presentation.ended?(nil); await presentation.recoveryRequested.wait()
    precondition(presentation.keep != nil && result.result == nil)
    // Graceful close has retired new presentation/capture admission. Keeping
    // these already owned snapshots must still reconcile their private ACKs.
    owner.invalidate(before: 2)
    presentation.keep?(); await task.value; result.success()
    let saves = await store.bases.count, discards = await discarder.calls
    precondition(saves == 0 && discards == 0, "Keep for Later published or discarded private edits")
    precondition(journal.files().map(\.url) == recovered.map(\.url))
    precondition(recovered.allSatisfy { journal.contains($0) }, "Keep closed without private retention ACKs")
    precondition(([viewer] + recovered).allSatisfy { !FileManager.default.fileExists(atPath: $0.directory.path) })
  }

  private static func failedKeepRetainsCopiesAndRetriesOnlyKeep() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(), journal = EditRetention(failures: 1), discarder = EditDiscard()
    let presentation = EditPresentation(), result = EditResult(), viewer = try fixture.file("viewer")
    let recovered = [try fixture.file("private-copy")]
    let context = ZoenFilePreview.EditContext(base: base, maximumBytes: 4096,
      retain: { try journal.retain($0) }, discard: { try await discarder.discard() }) { file, base in
        try await store.save(file, base)
      }
    let task = begin(owner, file: viewer, store: store, presentation: presentation,
      result: result, recovered: recovered, context: context)
    await presentation.shown.wait(); presentation.ended?(nil); await presentation.recoveryRequested.wait()
    let failed = EditSignal(); presentation.onRecovery = { failed.emit() }
    presentation.keep?(); await failed.wait()
    precondition(result.result == nil && presentation.recoveryCount == 2 && presentation.retry != nil)
    precondition(([viewer] + recovered).allSatisfy { FileManager.default.fileExists(atPath: $0.url.path) },
      "Failed Keep deleted an unacknowledged private copy")
    presentation.retry?(); await task.value; result.success()
    let saves = await store.bases.count, discards = await discarder.calls
    precondition(saves == 0 && discards == 0, "Keep retry switched to public Save or Discard")
    precondition(journal.files().count == 2 && journal.contains(recovered[0]))
    precondition(([viewer] + recovered).allSatisfy { !FileManager.default.fileExists(atPath: $0.directory.path) })
  }

  private static func recoveredReadOnlyViewerRejectsNewCaptures() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(), journal = EditRetention(), discarder = EditDiscard()
    let presentation = EditPresentation(), result = EditResult(), recovered = [try fixture.file("private-copy")]
    let context = ZoenFilePreview.EditContext(base: base, maximumBytes: 4096, canCapture: false,
      retain: { try journal.retain($0) }, discard: { try await discarder.discard() }) { file, base in
        try await store.save(file, base)
      }
    let task = begin(owner, file: try fixture.file("viewer"), store: store, presentation: presentation,
      result: result, recovered: recovered, context: context)
    await presentation.shown.wait()
    precondition(result.editing?.allowsEditing == false)
    let unexpected = try fixture.file("unexpected-framework-copy")
    result.editing!.receive(unexpected.url)
    precondition(journal.files().isEmpty && presentation.capturesFailed == 0,
      "Read-only recovered viewer admitted a new edited copy")
    presentation.ended?(nil); await presentation.recoveryRequested.wait()
    precondition(presentation.retry != nil && presentation.keep != nil && presentation.discard != nil,
      "Read-only recovered viewer lost explicit private recovery actions")
    presentation.keep?(); await task.value; result.success()
    precondition(journal.files().map(\.url) == recovered.map(\.url))
    let saves = await store.bases.count, discards = await discarder.calls
    precondition(saves == 0 && discards == 0)
    precondition(FileManager.default.fileExists(atPath: unexpected.url.path), "Rejected callback modified a framework-owned file")
  }
}
