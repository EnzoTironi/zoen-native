import Foundation

@MainActor extension FilePreviewTests {
  static func normalDismissalDeletesBeforeResolution() async throws {
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

  static func busyRejectsOnlyTheSecondLease() async throws {
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

  static func forcedClearWaitsForEntireHierarchy() async throws {
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

  static func queuedOldGenerationCannotPresent() async throws {
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

  static func failedPresentationCleansUpAndCanRetry() async throws {
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

}
