import Foundation

@main @MainActor struct FilePreviewTests {
  static func isFailure(_ error: Error, _ expected: ZoenFilePreview.Failure) -> Bool {
    switch (error as? ZoenFilePreview.Failure, expected) {
    case (.changed?, .changed), (.busy?, .busy), (.unavailable?, .unavailable), (.cleanup?, .cleanup): true
    default: false
    }
  }

  static func start(
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

}
