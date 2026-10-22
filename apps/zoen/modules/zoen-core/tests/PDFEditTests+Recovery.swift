import Foundation

@MainActor extension PDFEditTests {
  static func retryOlderCopyPreservesNewerCaptureFailure() async throws {
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

  static func closeRetiresRegisteredEditorBeforePresentation() async throws {
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

  static func everyCaptureRetainsSynchronouslyBeforePublicSave() async throws {
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

  static func recoveredCopiesWaitForExplicitSaveAndAdvanceBases() async throws {
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

}
