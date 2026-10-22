import CoreGraphics
import CryptoKit
import Foundation
import PDFKit

extension PDFEditJournalTests {
  static func lostCommitACKDoesNotRestageOrCreateAnotherVersion() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let a = try f.lease(PDFJournalFixture.pdf("commit-A", shade: 0.4))
    let b = try f.lease(PDFJournalFixture.pdf("commit-B", shade: 0.8))
    let store = PDFJournalStore(canonical: f.base), j = try journal(f, store)
    try j.retain(a); store.loseCommitACK()
    refused("Persisted commit ACK loss must surface") { _ = try j.save(a) }
    expect(store.currentBase().version == 8 && store.snapshot().pending.isEmpty)
    try j.retain(b)
    let bID = store.snapshot().pending[0].id
    let replay = try j.save(a)
    expect(replay.version == 8 && store.versionCount() == 1)
    expect(store.stages().count == 2 && store.snapshot().pending.map(\.id) == [bID],
                 "Accepted commit retry restaged A or erased newer B")
    expect(store.commits()[0] == store.commits()[1])
    expect(try j.save(b).version == 9)
    expect(store.versionCount() == 2)
  }

  static func recoveredQueuePinsReadsMapsExistingIDsAndCommitsFIFO() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let aBytes = try PDFJournalFixture.pdf("recovered-A", shade: 0.4)
    let bBytes = try PDFJournalFixture.pdf("recovered-B", shade: 0.8)
    let a = capture("capture_A", aBytes), b = capture("capture_B", bBytes)
    let base = PDFBase(version: 8, sha256: pdfDigest(try PDFJournalFixture.pdf("prior-accepted", shade: 0.5)))
    let store = PDFJournalStore(canonical: base)
    store.seed(.init(revision: "recovered_revision", editorID: "recovered_editor", origin: f.base,
                     base: base, pending: [a, b]), assets: [a.id: aBytes, b.id: bBytes])
    let j = try PDFJournal(observedBase: f.base, maximumBytes: journalMaximum, backend: store.backend())
    expect(j.origin == f.base && j.initialBase == base && j.editorID == "recovered_editor")
    expect(j.recovered.pending == [a, b])
    let aFile = try f.lease(j.recoveredBytes(a)), bFile = try f.lease(j.recoveredBytes(b))
    expect(store.reads() == ["recovered_revision:capture_A", "recovered_revision:capture_B"])
    try j.registerRecovered(bFile, capture: b); try j.registerRecovered(aFile, capture: a)
    expect(store.stages().isEmpty, "Materializing recovery must not stage new capture IDs")
    refused("Recovery must not publish the second capture before its head") { _ = try j.save(bFile) }
    expect(store.versionCount() == 0)
    expect(try j.save(aFile).version == 9)
    refused("A stale recovery read silently crossed journal revisions") { _ = try j.recoveredBytes(b) }
    expect(try j.save(bFile).version == 10)
    expect(store.stages().isEmpty && store.versionCount() == 2)
    expect(store.commits().map(\.capture) == [b.id, a.id, b.id])
  }

  static func changedCaptureAndWrongRecoveredCopyFailClosed() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let original = try PDFJournalFixture.pdf("retained-original", shade: 0.4)
    let replacement = try PDFJournalFixture.pdf("replacement", shade: 0.8)
    let file = try f.lease(original), store = PDFJournalStore(canonical: f.base), j = try journal(f, store)
    store.failStageBeforePersistence()
    refused("Initial storage failure must surface") { try j.retain(file) }
    try replacement.write(to: file.url, options: .atomic)
    refused("Changed bytes reused the original capture intent") { try j.retain(file) }
    expect(store.stages().count == 1 && store.snapshot().pending.isEmpty)
    let cap = capture("recovered_capture", original), recovered = PDFJournalStore(canonical: f.base)
    recovered.seed(.init(revision: "recovered", editorID: "editor", origin: f.base, base: f.base, pending: [cap]), assets: [cap.id: original])
    let recovering = try journal(f, recovered)
    refused("Different valid PDF was mapped to a recovered capture") { try recovering.registerRecovered(file, capture: cap) }
    expect(recovered.stages().isEmpty && recovered.commits().isEmpty)
  }

  static func corruptRecoveredAssetsRequireActualPDFHashAndSize() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let bytes = try PDFJournalFixture.pdf("asset", shade: 0.4), cap = capture("asset", bytes)
    let store = PDFJournalStore(canonical: f.base)
    store.seed(.init(revision: "recovered", editorID: "editor", origin: f.base, base: f.base, pending: [cap]), assets: [cap.id: bytes])
    let real = store.backend()
    for corrupted in [Data(bytes.dropLast()), try PDFJournalFixture.pdf("wrong-valid-PDF", shade: 0.8)] {
      let backend = PDFJournal.Backend(load: real.load, stage: real.stage,
        bytes: { revision, id in _ = try real.bytes(revision, id); return corrupted },
        commit: real.commit, discard: real.discard)
      let j = try journal(f, store, backend: backend)
      refused("Corrupt asset escaped exact size/SHA validation") { _ = try j.recoveredBytes(cap) }
    }
    let fake = Data("%PDF-1.7\nNot a parseable PDF\n%%EOF\n".utf8)
    let fakeCapture = capture("header_only", fake)
    let fakeState = PDFJournal.State(revision: "fake_pdf", editorID: "editor", origin: f.base, base: f.base, pending: [fakeCapture])
    let backend = PDFJournal.Backend(load: { fakeState }, stage: real.stage, bytes: { _, _ in fake }, commit: real.commit, discard: real.discard)
    let j = try journal(f, store, backend: backend)
    refused("Matching hash and PDF header must not replace PDFKit validation") { _ = try j.recoveredBytes(fakeCapture) }
  }

  static func unsafeOrUnboundedLeasesNeverReachStage() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let good = try f.lease(f.original), store = PDFJournalStore(canonical: f.base), j = try journal(f, store)
    let symlink = good.directory.appendingPathComponent("link.pdf")
    try FileManager.default.createSymbolicLink(at: symlink, withDestinationURL: good.url)
    let folderLink = f.root.appendingPathComponent("linked-folder", isDirectory: true)
    try FileManager.default.createSymbolicLink(at: folderLink, withDestinationURL: good.directory)
    let unsafe = [ZoenPreviewFile(url: symlink, directory: good.directory),
                  ZoenPreviewFile(url: folderLink.appendingPathComponent("synthetic.pdf"), directory: folderLink),
                  ZoenPreviewFile(url: good.url, directory: f.root),
                  ZoenPreviewFile(url: URL(string: "https://invalid.example/synthetic.pdf")!, directory: good.directory),
                  try f.lease(Data()), try f.lease(Data("%PDF-not-real".utf8))]
    for file in unsafe { refused("Unsafe/non-PDF source reached the private journal") { try j.retain(file) } }
    refused("Per-file byte limit was not enforced") { _ = try PDFJournal.read(good, maximumBytes: UInt64(f.original.count - 1)) }
    expect(store.stages().isEmpty && store.commits().isEmpty)
    expect(try Data(contentsOf: good.url) == f.original)
  }

}
