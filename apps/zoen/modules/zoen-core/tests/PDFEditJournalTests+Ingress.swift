import CoreGraphics
import CryptoKit
import Foundation
import PDFKit

extension PDFEditJournalTests {
  static func synchronousCaptureOwnsValidImmutablePDFBeforeCommit() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let edited = try PDFJournalFixture.pdf("edited-A", shade: 0.4)
    let frameworkSource = try f.lease(edited), owned = try f.capture(frameworkSource)
    let read = try PDFJournal.read(owned, maximumBytes: journalMaximum)
    expect(read == edited && PDFDocument(data: read)?.pageCount == 1)
    try f.original.write(to: frameworkSource.url, options: .atomic)
    try frameworkSource.remove()
    let store = PDFJournalStore(canonical: f.base), j = try journal(f, store)
    try j.retain(owned)
    expect(store.stages().count == 1 && store.commits().isEmpty,
                 "Retain must synchronously persist before a canonical version is attempted")
    expect(store.stages()[0].bytes == edited)
    let saved = try j.save(owned)
    expect(saved.version == 8 && saved.sha256 == pdfDigest(edited))
    expect(store.stages().count == 1 && store.versionCount() == 1)
    expect(try Data(contentsOf: owned.url) == edited, "Journal must never edit/remove its caller's lease")
  }

  static func lostStageACKRetriesExactIntentAfterNewerCapture() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let a = try f.lease(PDFJournalFixture.pdf("A", shade: 0.4))
    let b = try f.lease(PDFJournalFixture.pdf("B", shade: 0.8))
    let store = PDFJournalStore(canonical: f.base), j = try journal(f, store)
    store.loseStageACK()
    refused("Persisted stage ACK loss must surface") { try j.retain(a) }
    let first = store.stages()[0]
    expect(store.snapshot().pending.count == 1)
    try j.retain(b)
    let revisionWithB = store.snapshot().revision
    try j.retain(a)
    let calls = store.stages()
    expect(calls.count == 3 && calls[0] == calls[1], "A reconciliation changed its capture ID, bytes or expected revision")
    expect(calls[2].expected != first.expected && calls[2].capture != first.capture)
    expect(store.snapshot().revision == revisionWithB && store.snapshot().pending.map(\.id) == [first.capture, calls[2].capture])
    expect(try j.save(a).version == 8)
    expect(try j.save(b).version == 9)
    expect(store.stages().count == 3 && store.versionCount() == 2)
  }

  static func prewriteFailureReconcilesBeforeNewCaptureUsesRevision() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let a = try f.lease(PDFJournalFixture.pdf("unwritten-A", shade: 0.4))
    let b = try f.lease(PDFJournalFixture.pdf("newer-B", shade: 0.8))
    let store = PDFJournalStore(canonical: f.base), j = try journal(f, store)
    store.failStageBeforePersistence()
    refused("The prewrite failure must reach its caller") { try j.retain(a) }
    expect(store.snapshot().pending.isEmpty)
    try j.retain(b)
    let calls = store.stages()
    expect(calls.count == 3 && calls[0] == calls[1], "B stole A's still-unwritten CAS revision")
    expect(calls[2].expected != calls[0].expected)
    expect(store.snapshot().pending.map(\.id) == [calls[0].capture, calls[2].capture])
    expect(store.commits().isEmpty)
    expect(try j.save(a).version == 8)
    expect(try j.save(b).version == 9)
    expect(store.versionCount() == 2)
  }

  static func loadFailureCannotLetNewerCaptureOvertakeRetainedIngress() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let aBytes = try PDFJournalFixture.pdf("load-failed-A", shade: 0.4)
    let bBytes = try PDFJournalFixture.pdf("newer-after-load-B", shade: 0.8)
    let a = try f.lease(aBytes), b = try f.lease(bBytes)
    let store = PDFJournalStore(canonical: f.base), j = try journal(f, store)
    store.failNextLoad()
    refused("Failed revision read must surface without losing A admission") { try j.retain(a) }
    expect(store.stages().isEmpty)
    try j.retain(b)
    let calls = store.stages()
    expect(calls.map(\.bytes) == [aBytes, bBytes], "A valid retained ingress was forgotten when its revision load failed")
    expect(calls[0].expected == "0" && calls[1].expected != "0")
    expect(store.snapshot().pending.map(\.id) == calls.map(\.capture))
    expect(try j.save(a).version == 8)
    expect(try j.save(b).version == 9)
    expect(store.versionCount() == 2)
  }

  static func failedNewerIngressBlocksOlderCommitUntilReconciled() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let a = try f.lease(PDFJournalFixture.pdf("acknowledged-A", shade: 0.4))
    let b = try f.lease(PDFJournalFixture.pdf("unwritten-B", shade: 0.8))
    let store = PDFJournalStore(canonical: f.base), j = try journal(f, store)
    try j.retain(a)
    store.failStageBeforePersistence(times: 2)
    refused("Newer B ingress failure must surface") { try j.retain(b) }
    let bAttempt = store.stages()[1]
    refused("A committed while B still held its old private CAS revision") { _ = try j.save(a) }
    expect(store.commits().isEmpty && store.versionCount() == 0)
    expect(store.stages()[2] == bAttempt && store.snapshot().pending.count == 1)
    expect(FileManager.default.fileExists(atPath: a.url.path) && FileManager.default.fileExists(atPath: b.url.path))
    expect(try j.save(a).version == 8)
    expect(store.stages()[3] == bAttempt, "Retry rebased/reidentified the retained newer capture")
    expect(store.snapshot().pending.map(\.id) == [bAttempt.capture])
    expect(try j.save(b).version == 9)
    expect(store.versionCount() == 2 && store.stages().count == 4)
  }

}
