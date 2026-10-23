import CoreGraphics
import CryptoKit
import Foundation
import PDFKit

@main struct PDFEditJournalTests {
  static func expect(_ condition: Bool, _ message: String = "PDF journal assertion failed") {
    Swift.precondition(condition, message)
  }
  static func refused(_ message: String, _ action: () throws -> Void) {
    do { try action() }
    catch { return }
    preconditionFailure(message)
  }
  static func journal(_ fixture: PDFJournalFixture, _ store: PDFJournalStore,
                              backend: PDFJournal.Backend? = nil) throws -> PDFJournal {
    try .init(observedBase: fixture.base, maximumBytes: journalMaximum, backend: backend ?? store.backend())
  }
  static func capture(_ id: String, _ bytes: Data) -> PDFJournal.Capture {
    .init(id: id, sha256: pdfDigest(bytes), size: UInt64(bytes.count))
  }
  static func main() throws {
    try synchronousCaptureOwnsValidImmutablePDFBeforeCommit()
    try lostStageACKRetriesExactIntentAfterNewerCapture()
    try prewriteFailureReconcilesBeforeNewCaptureUsesRevision()
    try loadFailureCannotLetNewerCaptureOvertakeRetainedIngress()
    try failedNewerIngressBlocksOlderCommitUntilReconciled()
    try lostCommitACKDoesNotRestageOrCreateAnotherVersion()
    try recoveredQueuePinsReadsMapsExistingIDsAndCommitsFIFO()
    try changedCaptureAndWrongRecoveredCopyFailClosed()
    try corruptRecoveredAssetsRequireActualPDFHashAndSize()
    try unsafeOrUnboundedLeasesNeverReachStage()
    try failedDiscardAndForeignCASNeverClearNewerEditor()
    try successfulDiscardPermanentlyRetiresTheJournal()
    try emptyJournalStartsFreshWhileRecoveryKeepsOriginAndBase()
    try invalidDTOsAndAcknowledgmentsFailClosed()
    print("PDF edit journal tests passed: 14 scenarios; real one-page PDFKit/CoreGraphics fixtures, synchronous retention, exact lost-ACK replay, FIFO recovery, integrity/CAS and retirement.")
  }

}
