import Foundation

@MainActor extension PDFEditTests {
  static func recoveredTransfersCloseBeforePresentationAndDrain() async throws {
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

  static func failedRetainKeepsOwnedCopyForSaveRetry() async throws {
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

  static func failedAsyncDiscardRetriesOnlyDiscard() async throws {
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

  static func forcedClearPreservesRecoveredJournal() async throws {
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

  static func forcedClearDrainsAdmittedDiscard() async throws {
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

  static func partialTransfersAbandonOnceAndYieldToClear() async throws {
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

}
