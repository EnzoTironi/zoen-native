import Foundation

@MainActor extension FilePreviewTests {
  static func deniedDeletionIsRetainedAndRetryable() async throws {
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

  static func clearOwnsTransferBeforeQueuedPresentationBegins() async throws {
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

  static func deniedPendingTransferRemainsOwnedUntilClearRetry() async throws {
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

}
