import Foundation

@MainActor private final class PreviewPresentation: ZoenFilePreviewPresentation {
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

@MainActor private final class PreviewCompletion {
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

@MainActor private final class PreviewSignal {
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

@MainActor private final class PreviewFixture {
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

@main @MainActor struct FilePreviewTests {
  private static func isFailure(_ error: Error, _ expected: ZoenFilePreview.Failure) -> Bool {
    switch (error as? ZoenFilePreview.Failure, expected) {
    case (.changed?, .changed), (.busy?, .busy), (.unavailable?, .unavailable), (.cleanup?, .cleanup): true
    default: false
    }
  }

  private static func start(
    _ coordinator: ZoenFilePreview, file: ZoenPreviewFile, generation: UInt64 = 1,
    presentation: PreviewPresentation, completion: PreviewCompletion
  ) -> Task<Void, Never> {
    let id = coordinator.register(file: file, generation: generation)
    return Task { @MainActor in
      await completion.run {
        try await coordinator.open(id: id, title: "Synthetic signed document") { presentation }
      }
    }
  }

  static func main() async throws {
    try await normalDismissalDeletesBeforeResolution()
    try await busyRejectsOnlyTheSecondLease()
    try await forcedClearWaitsForEntireHierarchy()
    try await queuedOldGenerationCannotPresent()
    try await failedPresentationCleansUpAndCanRetry()
    try await taskCancellationWaitsForDismissal()
    try await staleEndedCallbackCannotFinishNewPreview()
    try await deniedDeletionIsRetainedAndRetryable()
    try await clearOwnsTransferBeforeQueuedPresentationBegins()
    try await deniedPendingTransferRemainsOwnedUntilClearRetry()
    try validatedLeaseAcceptsCacheAliasesAndRejectsSubstitutedFiles()
    print("Synthetic file preview tests passed: 11 scenarios; native dismissal/deletion ordering, reentrancy, concurrent forced hierarchy completion, generation invalidation, failure/retry, cancellation, stale callbacks, transferred pending leases, denied active/pending cleanup and retry, canonical cache aliases and exact lease validation.")
  }

  private static func normalDismissalDeletesBeforeResolution() async throws {
    let fixture = try PreviewFixture()
    defer { fixture.cleanup() }
    let file = try fixture.file("normal")
    let coordinator = ZoenFilePreview()
    let presentation = PreviewPresentation()
    let completion = PreviewCompletion()
    completion.onCompletion = { precondition(!fixture.exists(file), "Preview resolved while plaintext still exists") }
    let task = start(coordinator, file: file, presentation: presentation, completion: completion)
    await presentation.waitUntilShown()
    precondition(presentation.shownFile == file.url)
    precondition(presentation.shownTitle == "Synthetic signed document")
    precondition(fixture.exists(file))
    precondition(!completion.finished)
    presentation.endNormally()
    await task.value
    completion.expectSuccess()
    precondition(presentation.dismissCount == 0)
    precondition(!fixture.exists(file))
  }

  private static func busyRejectsOnlyTheSecondLease() async throws {
    let fixture = try PreviewFixture()
    defer { fixture.cleanup() }
    let firstFile = try fixture.file("busy-first")
    let secondFile = try fixture.file("busy-second")
    let coordinator = ZoenFilePreview()
    let presentation = PreviewPresentation()
    let first = PreviewCompletion()
    let firstTask = start(coordinator, file: firstFile, presentation: presentation, completion: first)
    await presentation.waitUntilShown()
    let rejectedPresentation = PreviewPresentation()
    let second = PreviewCompletion()
    let secondTask = start(coordinator, file: secondFile, presentation: rejectedPresentation, completion: second)
    await secondTask.value
    second.expectFailure { isFailure($0, .busy) }
    precondition(rejectedPresentation.shownFile == nil)
    precondition(!fixture.exists(secondFile))
    precondition(fixture.exists(firstFile), "Busy rejection deleted the active preview's lease")
    precondition(!first.finished)
    presentation.endNormally()
    await firstTask.value
    first.expectSuccess()
  }

  private static func forcedClearWaitsForEntireHierarchy() async throws {
    let fixture = try PreviewFixture()
    defer { fixture.cleanup() }
    let file = try fixture.file("forced-clear")
    let coordinator = ZoenFilePreview()
    let presentation = PreviewPresentation()
    let opened = PreviewCompletion()
    let opening = start(coordinator, file: file, presentation: presentation, completion: opened)
    await presentation.waitUntilShown()
    let cleared = PreviewCompletion()
    cleared.onCompletion = { precondition(!fixture.exists(file), "Clear resolved before copy deletion") }
    let clearing = Task { @MainActor in
      await cleared.run { try await coordinator.clear(before: 2) }
    }
    await presentation.waitUntilDismissRequested()
    let alsoCleared = PreviewCompletion()
    alsoCleared.onCompletion = { precondition(!fixture.exists(file), "Concurrent clear resolved before copy deletion") }
    let secondEntered = PreviewSignal()
    let alsoClearing = Task { @MainActor in
      await alsoCleared.run {
        // The same actor enters clear without another suspension after this
        // signal; its already-stopping session must register another waiter.
        secondEntered.emit()
        try await coordinator.clear(before: 2)
      }
    }
    await secondEntered.wait()
    precondition(presentation.dismissCount == 1)
    precondition(!opened.finished && !cleared.finished && !alsoCleared.finished)
    precondition(fixture.exists(file))
    // UIKit may report the preview's normal callback before a presented share
    // controller's forced dismissal has finished. That callback cannot win.
    presentation.endNormally()
    precondition(!opened.finished && !cleared.finished && !alsoCleared.finished)
    precondition(fixture.exists(file))
    presentation.completeHierarchyDismissal()
    await opening.value
    await clearing.value
    await alsoClearing.value
    opened.expectFailure { isFailure($0, .changed) }
    cleared.expectSuccess()
    alsoCleared.expectSuccess()
    precondition(!fixture.exists(file))
  }

  private static func queuedOldGenerationCannotPresent() async throws {
    let fixture = try PreviewFixture()
    defer { fixture.cleanup() }
    let coordinator = ZoenFilePreview()
    let oldFile = try fixture.file("old-generation")
    let oldPresentation = PreviewPresentation()
    let old = PreviewCompletion()
    let queued = start(coordinator, file: oldFile, generation: 1, presentation: oldPresentation, completion: old)
    // No actor suspension occurs before this nonisolated invalidation. The
    // queued MainActor task therefore encounters the new generation floor.
    coordinator.invalidate(before: 2)
    await queued.value
    old.expectFailure { isFailure($0, .changed) }
    precondition(oldPresentation.shownFile == nil)
    precondition(!fixture.exists(oldFile))
    let currentFile = try fixture.file("current-generation")
    let currentPresentation = PreviewPresentation()
    let current = PreviewCompletion()
    let opening = start(coordinator, file: currentFile, generation: 2, presentation: currentPresentation, completion: current)
    await currentPresentation.waitUntilShown()
    currentPresentation.endNormally()
    await opening.value
    current.expectSuccess()
    precondition(!fixture.exists(currentFile))
  }

  private static func failedPresentationCleansUpAndCanRetry() async throws {
    let fixture = try PreviewFixture()
    defer { fixture.cleanup() }
    let coordinator = ZoenFilePreview()
    let rejectedFile = try fixture.file("presentation-failure")
    let rejectedPresentation = PreviewPresentation()
    rejectedPresentation.failShow = true
    let rejected = PreviewCompletion()
    let failing = start(coordinator, file: rejectedFile, presentation: rejectedPresentation, completion: rejected)
    await failing.value
    rejected.expectFailure { $0 is PreviewPresentation.PresentationFailure }
    precondition(!fixture.exists(rejectedFile))
    precondition(rejectedPresentation.dismissCount == 0)
    let retryFile = try fixture.file("presentation-retry")
    let retryPresentation = PreviewPresentation()
    let retried = PreviewCompletion()
    let retrying = start(coordinator, file: retryFile, presentation: retryPresentation, completion: retried)
    await retryPresentation.waitUntilShown()
    retryPresentation.endNormally()
    await retrying.value
    retried.expectSuccess()
  }

  private static func taskCancellationWaitsForDismissal() async throws {
    let fixture = try PreviewFixture()
    defer { fixture.cleanup() }
    let file = try fixture.file("cancelled")
    let coordinator = ZoenFilePreview()
    let presentation = PreviewPresentation()
    let completion = PreviewCompletion()
    completion.onCompletion = { precondition(!fixture.exists(file), "Cancelled preview resolved before deletion") }
    let opening = start(coordinator, file: file, presentation: presentation, completion: completion)
    await presentation.waitUntilShown()
    opening.cancel()
    await presentation.waitUntilDismissRequested()
    precondition(presentation.dismissCount == 1)
    precondition(!completion.finished && fixture.exists(file))
    presentation.endNormally()
    precondition(!completion.finished && fixture.exists(file))
    presentation.completeHierarchyDismissal()
    await opening.value
    completion.expectFailure { $0 is CancellationError }
    precondition(!fixture.exists(file))
  }

  private static func staleEndedCallbackCannotFinishNewPreview() async throws {
    let fixture = try PreviewFixture()
    defer { fixture.cleanup() }
    let coordinator = ZoenFilePreview()
    let oldFile = try fixture.file("stale-old")
    let oldPresentation = PreviewPresentation()
    let old = PreviewCompletion()
    let oldTask = start(coordinator, file: oldFile, presentation: oldPresentation, completion: old)
    await oldPresentation.waitUntilShown()
    oldPresentation.endNormally()
    await oldTask.value
    old.expectSuccess()
    let newFile = try fixture.file("stale-new")
    let newPresentation = PreviewPresentation()
    let new = PreviewCompletion()
    let newTask = start(coordinator, file: newFile, presentation: newPresentation, completion: new)
    await newPresentation.waitUntilShown()
    oldPresentation.endNormally(error: PreviewPresentation.PresentationFailure.unavailable)
    oldPresentation.endNormally()
    precondition(!new.finished)
    precondition(fixture.exists(newFile))
    precondition(newPresentation.dismissCount == 0)
    newPresentation.endNormally()
    await newTask.value
    new.expectSuccess()
    precondition(!fixture.exists(newFile))
  }

  private static func deniedDeletionIsRetainedAndRetryable() async throws {
    let fixture = try PreviewFixture()
    defer { fixture.cleanup() }
    let file = try fixture.file("denied-cleanup")
    let coordinator = ZoenFilePreview()
    let presentation = PreviewPresentation()
    let completion = PreviewCompletion()
    let opening = start(coordinator, file: file, presentation: presentation, completion: completion)
    await presentation.waitUntilShown()
    try FileManager.default.setAttributes([.posixPermissions: 0o000], ofItemAtPath: file.directory.path)
    defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: file.directory.path) }
    presentation.endNormally()
    await opening.value
    completion.expectFailure {
      guard let error = $0 as? CocoaError else { return false }
      return error.code == .fileReadNoPermission || error.code == .fileWriteNoPermission
    }
    _ = try FileManager.default.attributesOfItem(atPath: file.directory.path)
    do {
      try await coordinator.clear(before: 2)
      preconditionFailure("Denied deletion was treated as completed cleanup")
    } catch {}
    let refusedFile = try fixture.file("blocked-by-retained-copy")
    let refusedPresentation = PreviewPresentation()
    let refused = PreviewCompletion()
    let refusedTask = start(coordinator, file: refusedFile, generation: 2, presentation: refusedPresentation, completion: refused)
    await refusedTask.value
    refused.expectFailure { isFailure($0, .busy) }
    precondition(refusedPresentation.shownFile == nil)
    precondition(!fixture.exists(refusedFile))
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: file.directory.path)
    precondition(fixture.exists(file), "Denied copy disappeared without a successful cleanup")
    try await coordinator.clear(before: 2)
    precondition(!fixture.exists(file))
    let retryFile = try fixture.file("after-cleanup-retry")
    let retryPresentation = PreviewPresentation()
    let retry = PreviewCompletion()
    let retrying = start(coordinator, file: retryFile, generation: 2, presentation: retryPresentation, completion: retry)
    await retryPresentation.waitUntilShown()
    retryPresentation.endNormally()
    await retrying.value
    retry.expectSuccess()
  }

  private static func clearOwnsTransferBeforeQueuedPresentationBegins() async throws {
    let fixture = try PreviewFixture()
    defer { fixture.cleanup() }
    let coordinator = ZoenFilePreview()
    let oldFile = try fixture.file("transferred-before-main-actor")
    let oldTicket = coordinator.register(file: oldFile, generation: 1)
    let currentFile = try fixture.file("pending-current-generation")
    let currentTicket = coordinator.register(file: currentFile, generation: 2)
    let cleared = PreviewCompletion()
    cleared.onCompletion = {
      precondition(!fixture.exists(oldFile), "Clear returned with an older transferred preview still on disk")
      precondition(fixture.exists(currentFile), "Clear removed a current-generation transfer")
    }
    // No open call has entered MainActor yet. The registered transfer alone
    // must make this cleanup authoritative over the exact plaintext directory.
    await cleared.run { try await coordinator.clear(before: 2) }
    cleared.expectSuccess()
    let oldPresentation = PreviewPresentation()
    let late = PreviewCompletion()
    await late.run {
      try await coordinator.open(id: oldTicket, title: "Retired transfer") { oldPresentation }
    }
    late.expectFailure { isFailure($0, .changed) }
    precondition(oldPresentation.shownFile == nil)
    precondition(!fixture.exists(oldFile))
    let currentPresentation = PreviewPresentation()
    let current = PreviewCompletion()
    let opening = Task { @MainActor in
      await current.run {
        try await coordinator.open(id: currentTicket, title: "Current transfer") { currentPresentation }
      }
    }
    await currentPresentation.waitUntilShown()
    precondition(fixture.exists(currentFile))
    currentPresentation.endNormally()
    await opening.value
    current.expectSuccess()
    precondition(!fixture.exists(currentFile))
  }

  private static func deniedPendingTransferRemainsOwnedUntilClearRetry() async throws {
    let fixture = try PreviewFixture()
    defer { fixture.cleanup() }
    let coordinator = ZoenFilePreview()
    let pendingFile = try fixture.file("denied-transferred-copy")
    let ticket = coordinator.register(file: pendingFile, generation: 1)
    try FileManager.default.setAttributes([.posixPermissions: 0o000], ofItemAtPath: pendingFile.directory.path)
    defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: pendingFile.directory.path) }
    let cleared = PreviewCompletion()
    await cleared.run { try await coordinator.clear(before: 2) }
    cleared.expectFailure {
      guard let error = $0 as? CocoaError else { return false }
      return error.code == .fileReadNoPermission || error.code == .fileWriteNoPermission
    }
    _ = try FileManager.default.attributesOfItem(atPath: pendingFile.directory.path)
    let latePresentation = PreviewPresentation()
    let late = PreviewCompletion()
    await late.run {
      try await coordinator.open(id: ticket, title: "Denied retired transfer") { latePresentation }
    }
    late.expectFailure { isFailure($0, .changed) }
    precondition(latePresentation.shownFile == nil)
    _ = try FileManager.default.attributesOfItem(atPath: pendingFile.directory.path)
    let refusedFile = try fixture.file("pending-retained-blocks-reentry")
    let refusedPresentation = PreviewPresentation()
    let refused = PreviewCompletion()
    let refusedTask = start(coordinator, file: refusedFile, generation: 2, presentation: refusedPresentation, completion: refused)
    await refusedTask.value
    refused.expectFailure { isFailure($0, .busy) }
    precondition(!fixture.exists(refusedFile))
    precondition(refusedPresentation.shownFile == nil)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: pendingFile.directory.path)
    precondition(fixture.exists(pendingFile), "A late open lost the retained transferred lease")
    try await coordinator.clear(before: 2)
    precondition(!fixture.exists(pendingFile))
    let retryFile = try fixture.file("after-pending-clear-retry")
    let retryPresentation = PreviewPresentation()
    let retry = PreviewCompletion()
    let retrying = start(coordinator, file: retryFile, generation: 2, presentation: retryPresentation, completion: retry)
    await retryPresentation.waitUntilShown()
    retryPresentation.endNormally()
    await retrying.value
    retry.expectSuccess()
    precondition(!fixture.exists(retryFile))
  }

  private static func validatedLeaseAcceptsCacheAliasesAndRejectsSubstitutedFiles() throws {
    let fixture = try PreviewFixture()
    defer { fixture.cleanup() }
    let manager = FileManager.default
    let sandbox = fixture.root.resolvingSymlinksInPath().standardizedFileURL
    let cache = sandbox.appendingPathComponent("Library/Caches", isDirectory: true)
    let namespace = cache.appendingPathComponent("ZoenMedia", isDirectory: true)
    let ownedDirectory = namespace.appendingPathComponent(UUID().uuidString, isDirectory: true)
    let siblingDirectory = namespace.appendingPathComponent(UUID().uuidString, isDirectory: true)
    let externalDirectory = sandbox.appendingPathComponent("ArbitraryExternalOriginal", isDirectory: true)
    let externalBytes = Data("Synthetic external original must survive".utf8)

    func write(_ directory: URL, name: String = "synthetic.pdf", bytes: Data = Data("Synthetic validated lease".utf8)) throws -> URL {
      try manager.createDirectory(at: directory, withIntermediateDirectories: true)
      let url = directory.appendingPathComponent(name)
      try bytes.write(to: url)
      return url
    }
    func reject(_ url: URL, directory: URL, maximumBytes: UInt64 = 4096) {
      do {
        _ = try ZoenPreviewFile.validated(url: url, directory: directory, cacheRoot: cache, maximumBytes: maximumBytes)
        preconditionFailure("A substituted or invalid lease was accepted: \(url.lastPathComponent)")
      } catch ZoenFilePreview.Failure.unavailable {
      } catch {
        preconditionFailure("Unexpected lease validation error: \(error)")
      }
    }

    let ownedURL = try write(ownedDirectory)
    let siblingURL = try write(siblingDirectory)
    let externalURL = try write(externalDirectory, bytes: externalBytes)
    let plain = try ZoenPreviewFile.validated(url: ownedURL, directory: ownedDirectory, cacheRoot: cache, maximumBytes: 4096)
    precondition(plain.url == ownedURL)
    precondition(plain.directory == ownedDirectory)

    // Model iOS's legitimate /var -> /private/var ancestor alias with a real
    // filesystem symlink, without permitting a link at the generated leaf.
    let aliasCache = sandbox.appendingPathComponent("OSCacheAncestorAlias", isDirectory: true)
    try manager.createSymbolicLink(at: aliasCache, withDestinationURL: cache)
    let aliasDirectory = aliasCache.appendingPathComponent("ZoenMedia", isDirectory: true)
      .appendingPathComponent(ownedDirectory.lastPathComponent, isDirectory: true)
    let aliasURL = aliasDirectory.appendingPathComponent(ownedURL.lastPathComponent)
    for cacheSpelling in [cache, aliasCache] {
      let accepted = try ZoenPreviewFile.validated(url: aliasURL, directory: aliasDirectory, cacheRoot: cacheSpelling, maximumBytes: 4096)
      precondition(accepted.url == plain.url && accepted.directory == plain.directory)
    }

    reject(ownedURL, directory: siblingDirectory)
    let nestedURL = try write(ownedDirectory.appendingPathComponent("nested", isDirectory: true))
    reject(nestedURL, directory: ownedDirectory)
    let outsideDirectory = sandbox.appendingPathComponent("OutsideNamespace", isDirectory: true)
      .appendingPathComponent(UUID().uuidString, isDirectory: true)
    let outsideURL = try write(outsideDirectory)
    reject(outsideURL, directory: outsideDirectory)
    let nonGeneratedDirectory = namespace.appendingPathComponent("not-a-generated-uuid", isDirectory: true)
    let nonGeneratedURL = try write(nonGeneratedDirectory)
    reject(nonGeneratedURL, directory: nonGeneratedDirectory)

    let leafLink = ownedDirectory.appendingPathComponent("external-original.pdf")
    try manager.createSymbolicLink(at: leafLink, withDestinationURL: externalURL)
    reject(leafLink, directory: ownedDirectory)
    // Point at another otherwise valid namespace leaf so rejection specifically
    // exercises the generated-root link check, not just namespace containment.
    let rootLink = namespace.appendingPathComponent(UUID().uuidString, isDirectory: true)
    try manager.createSymbolicLink(at: rootLink, withDestinationURL: siblingDirectory)
    reject(rootLink.appendingPathComponent(siblingURL.lastPathComponent), directory: rootLink)
    reject(ownedURL, directory: ownedDirectory, maximumBytes: 1)
    let emptyURL = try write(ownedDirectory, name: "empty.pdf", bytes: Data())
    reject(emptyURL, directory: ownedDirectory)

    try plain.remove()
    precondition(!manager.fileExists(atPath: ownedDirectory.path))
    let preservedExternalBytes = try Data(contentsOf: externalURL)
    precondition(preservedExternalBytes == externalBytes, "Lease cleanup followed a link into an external original")
    precondition(manager.fileExists(atPath: siblingURL.path), "Lease cleanup deleted another generated directory")
    precondition(manager.fileExists(atPath: outsideURL.path))
    precondition(manager.fileExists(atPath: nonGeneratedURL.path))
    let aliasType = try manager.attributesOfItem(atPath: aliasCache.path)[.type] as? FileAttributeType
    precondition(aliasType == .typeSymbolicLink)
  }
}
