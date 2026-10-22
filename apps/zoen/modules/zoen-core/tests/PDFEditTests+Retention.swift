import Foundation

@MainActor extension PDFEditTests {
  static func keepForLaterRetainsWithoutPublishingOrDiscarding() async throws {
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

  static func failedKeepRetainsCopiesAndRetriesOnlyKeep() async throws {
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

  static func recoveredReadOnlyViewerRejectsNewCaptures() async throws {
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
