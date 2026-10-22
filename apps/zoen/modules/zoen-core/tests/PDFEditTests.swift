import Foundation

@main @MainActor struct PDFEditTests {
  static let base = ZoenFilePreview.EditBase(version: 7, sha256: "canonical-seven")
  static func begin(_ owner: ZoenFilePreview, file: ZoenPreviewFile, store: EditStore,
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

}
