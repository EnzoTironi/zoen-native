import Foundation

@MainActor extension PDFEditTests {
  static func serialSavesOwnTheirCopiesAndAdvanceBase() async throws {
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

  static func failedSaveRetainsCopyUntilRetry() async throws {
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

  static func discardIsExplicitAndDoesNotRetry() async throws {
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

  static func forcedCleanupWaitsWorkerAndDoesNotSaveQueuedCopy() async throws {
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

  static func rowCancellationDoesNotDiscardMarkup() async throws {
    let fixture = try EditFixture(); defer { fixture.remove() }
    let owner = ZoenFilePreview(), store = EditStore(), presentation = EditPresentation(), result = EditResult()
    let task = begin(owner, file: try fixture.file("original"), store: store, presentation: presentation, result: result)
    await presentation.shown.wait(); task.cancel(); await Task.yield(); await Task.yield()
    precondition(presentation.dismissCount == 0 && result.result == nil)
    presentation.ended?(nil); await task.value; result.success()
  }

  static func acceptedSaveCleanupRetryDoesNotSaveAgain() async throws {
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

  static func captureFailureRequiresAcknowledgedDiscard() async throws {
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

  static func captureRejectsUnsafeOrUnboundedSources() throws {
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
