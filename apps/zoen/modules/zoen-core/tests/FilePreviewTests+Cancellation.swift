import Foundation

@MainActor extension FilePreviewTests {
  static func taskCancellationWaitsForDismissal() async throws {
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

  static func staleEndedCallbackCannotFinishNewPreview() async throws {
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

}
