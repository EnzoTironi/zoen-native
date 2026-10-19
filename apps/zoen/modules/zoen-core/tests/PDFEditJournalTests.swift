import CoreGraphics
import CryptoKit
import Foundation
import PDFKit

private typealias PDFJournal = ZoenPDFEditJournal
private typealias PDFBase = ZoenFilePreview.EditBase
private let journalMaximum: UInt64 = 64 << 20

private func pdfDigest(_ bytes: Data) -> String {
  SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
}

/// Synchronous encrypted-core port model: receipts are exact intents, commits
/// consume only the FIFO head, and read/discard require the observed revision.
/// It stores a physical copy of ingress bytes, as Rust does before returning.
private final class PDFJournalStore: @unchecked Sendable {
  enum Fault: Error { case offline, ambiguousStage, ambiguousCommit, stale, invalid }
  struct StageCall: Equatable, Sendable {
    let expected: String
    let editor: String
    let capture: String
    let origin: PDFBase
    let bytes: Data
  }
  struct CommitCall: Equatable, Sendable { let editor: String; let capture: String }
  private struct StageReceipt { let call: StageCall; let accepted: String }
  private struct CommitReceipt { let call: CommitCall; let result: PDFBase }
  private let lock = NSRecursiveLock()
  private var value = PDFJournal.State(revision: "0", editorID: nil, origin: nil, base: nil, pending: [])
  private var canonical: PDFBase
  private var assets: [String: Data] = [:]
  private var stageReceipts: [String: StageReceipt] = [:]
  private var commitReceipts: [String: CommitReceipt] = [:]
  private var stageLog: [StageCall] = []
  private var commitLog: [CommitCall] = []
  private var readLog: [String] = []
  private var discardLog: [String] = []
  private var versions = 0
  private var sequence = 0
  private var loadFailures = 0
  private var stageBeforeFailures = 0
  private var stageACKFailure = false
  private var commitACKFailure = false
  private var discardBeforeFailure = false
  private var foreignBeforeDiscard: Data?

  init(canonical: PDFBase) { self.canonical = canonical }
  private func locked<T>(_ action: () throws -> T) rethrows -> T {
    lock.lock(); defer { lock.unlock() }; return try action()
  }
  private func revision() -> String { sequence += 1; return "revision_\(sequence)" }
  private func copy(_ bytes: Data) -> Data { Data(Array(bytes)) }
  func snapshot() -> PDFJournal.State { locked { value } }
  func stages() -> [StageCall] { locked { stageLog } }
  func commits() -> [CommitCall] { locked { commitLog } }
  func reads() -> [String] { locked { readLog } }
  func discards() -> [String] { locked { discardLog } }
  func versionCount() -> Int { locked { versions } }
  func currentBase() -> PDFBase { locked { canonical } }
  func failNextLoad() { locked { loadFailures += 1 } }
  func failStageBeforePersistence(times: Int = 1) { locked { stageBeforeFailures += times } }
  func loseStageACK() { locked { stageACKFailure = true } }
  func loseCommitACK() { locked { commitACKFailure = true } }
  func failDiscardBeforePersistence() { locked { discardBeforeFailure = true } }
  func raceForeignCaptureBeforeDiscard(_ bytes: Data) { locked { foreignBeforeDiscard = copy(bytes) } }
  func seed(_ state: PDFJournal.State, assets: [String: Data]) {
    locked {
      value = state
      self.assets = assets.mapValues { copy($0) }
    }
  }
  private func load() throws -> PDFJournal.State {
    try locked {
      if loadFailures > 0 { loadFailures -= 1; throw Fault.offline }
      return value
    }
  }
  private func stage(_ request: PDFJournal.Stage) throws -> PDFJournal.Acknowledgment {
    try locked {
      let call = StageCall(expected: request.expectedRevision, editor: request.editorID,
                           capture: request.captureID, origin: request.origin, bytes: copy(request.bytes))
      stageLog.append(call)
      if let receipt = stageReceipts[call.expected] {
        guard receipt.call == call else { throw Fault.stale }
        return .init(acceptedRevision: receipt.accepted, draft: value)
      }
      if stageBeforeFailures > 0 { stageBeforeFailures -= 1; throw Fault.offline }
      guard call.expected == value.revision,
            !value.pending.contains(where: { $0.id == call.capture }),
            commitReceipts[call.capture] == nil else { throw Fault.stale }
      let origin: PDFBase
      let base: PDFBase
      if value.editorID == call.editor {
        guard value.origin == call.origin, let currentBase = value.base else { throw Fault.stale }
        origin = call.origin; base = currentBase
      } else {
        guard value.pending.isEmpty, call.origin == canonical else { throw Fault.stale }
        origin = call.origin; base = canonical
      }
      let capture = PDFJournal.Capture(id: call.capture, sha256: pdfDigest(call.bytes), size: UInt64(call.bytes.count))
      value = .init(revision: revision(), editorID: call.editor, origin: origin, base: base,
                    pending: value.pending + [capture])
      assets[capture.id] = copy(call.bytes)
      stageReceipts[call.expected] = StageReceipt(call: call, accepted: value.revision)
      if stageACKFailure { stageACKFailure = false; throw Fault.ambiguousStage }
      return .init(acceptedRevision: value.revision, draft: value)
    }
  }
  private func bytes(_ expected: String, _ capture: String) throws -> Data {
    try locked {
      readLog.append("\(expected):\(capture)")
      guard value.revision == expected, value.pending.contains(where: { $0.id == capture }),
            let bytes = assets[capture] else { throw Fault.stale }
      return copy(bytes)
    }
  }
  private func commit(_ editor: String, _ capture: String) throws -> PDFBase {
    try locked {
      let call = CommitCall(editor: editor, capture: capture)
      commitLog.append(call)
      if let receipt = commitReceipts[capture] {
        guard receipt.call == call else { throw Fault.stale }
        return receipt.result
      }
      guard value.editorID == editor, value.pending.first?.id == capture,
            value.base == canonical, let origin = value.origin,
            let bytes = assets[capture], let head = value.pending.first,
            head.sha256 == pdfDigest(bytes), head.size == UInt64(bytes.count) else { throw Fault.stale }
      canonical = .init(version: canonical.version + 1, sha256: pdfDigest(bytes))
      versions += 1
      commitReceipts[capture] = CommitReceipt(call: call, result: canonical)
      value = .init(revision: revision(), editorID: editor, origin: origin, base: canonical,
                    pending: Array(value.pending.dropFirst()))
      assets.removeValue(forKey: capture)
      if commitACKFailure { commitACKFailure = false; throw Fault.ambiguousCommit }
      return canonical
    }
  }
  private func discard(_ expected: String) throws -> PDFJournal.State {
    try locked {
      discardLog.append(expected)
      if let bytes = foreignBeforeDiscard {
        foreignBeforeDiscard = nil
        let capture = PDFJournal.Capture(id: "foreign_capture", sha256: pdfDigest(bytes), size: UInt64(bytes.count))
        assets = [capture.id: copy(bytes)]
        value = .init(revision: revision(), editorID: "foreign_editor", origin: canonical,
                      base: canonical, pending: [capture])
      }
      guard value.revision == expected else { throw Fault.stale }
      if discardBeforeFailure { discardBeforeFailure = false; throw Fault.offline }
      assets.removeAll()
      value = .init(revision: revision(), editorID: value.editorID, origin: value.origin,
                    base: value.base, pending: [])
      return value
    }
  }
  func backend() -> PDFJournal.Backend {
    .init(load: { try self.load() }, stage: { try self.stage($0) },
          bytes: { try self.bytes($0, $1) }, commit: { try self.commit($0, $1) },
          discard: { try self.discard($0) })
  }
}

private final class PDFJournalFixture {
  let root: URL
  let original: Data
  private var transferred: [ZoenPreviewFile] = []
  var base: PDFBase { .init(version: 7, sha256: pdfDigest(original)) }
  init() throws {
    root = FileManager.default.temporaryDirectory.appendingPathComponent("zoen-pdf-journal-test-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true,
                                          attributes: [.posixPermissions: 0o700])
    original = try Self.pdf("canonical-original", shade: 0.2)
  }
  static func pdf(_ title: String, shade: CGFloat) throws -> Data {
    let output = NSMutableData()
    var box = CGRect(x: 0, y: 0, width: 144, height: 144)
    guard let consumer = CGDataConsumer(data: output as CFMutableData),
          let context = CGContext(consumer: consumer, mediaBox: &box,
                                  [kCGPDFContextTitle as String: title] as CFDictionary) else {
      throw PDFJournalStore.Fault.invalid
    }
    context.beginPDFPage(nil)
    context.setFillColor(red: shade, green: 0.3, blue: 0.7, alpha: 1)
    context.fill(CGRect(x: 12, y: 12, width: 80, height: 60))
    context.endPDFPage(); context.closePDF()
    let bytes = Data(bytes: output.bytes, count: output.length)
    guard bytes.starts(with: Data("%PDF-".utf8)), let document = PDFDocument(data: bytes),
          document.pageCount == 1, !document.isLocked,
          document.page(at: 0)?.bounds(for: .mediaBox).width == 144 else { throw PDFJournalStore.Fault.invalid }
    return bytes
  }
  func lease(_ bytes: Data) throws -> ZoenPreviewFile {
    let directory = root.appendingPathComponent(UUID().uuidString, isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true,
                                          attributes: [.posixPermissions: 0o700])
    let file = ZoenPreviewFile(url: directory.appendingPathComponent("synthetic.pdf"), directory: directory)
    try bytes.write(to: file.url, options: .atomic)
    return file
  }
  func capture(_ source: ZoenPreviewFile) throws -> ZoenPreviewFile {
    switch ZoenPDFEditCapture.stage(source.url, maximumBytes: journalMaximum) {
    case .success(let file): transferred.append(file); return file
    case .failure(let error):
      if let file = error.retained { transferred.append(file) }
      throw error
    }
  }
  func remove() {
    for file in transferred { try? file.remove() }
    try? FileManager.default.removeItem(at: root)
  }
}

@main struct PDFEditJournalTests {
  private static func expect(_ condition: Bool, _ message: String = "PDF journal assertion failed") {
    Swift.precondition(condition, message)
  }
  private static func refused(_ message: String, _ action: () throws -> Void) {
    do { try action() }
    catch { return }
    preconditionFailure(message)
  }
  private static func journal(_ fixture: PDFJournalFixture, _ store: PDFJournalStore,
                              backend: PDFJournal.Backend? = nil) throws -> PDFJournal {
    try .init(observedBase: fixture.base, maximumBytes: journalMaximum, backend: backend ?? store.backend())
  }
  private static func capture(_ id: String, _ bytes: Data) -> PDFJournal.Capture {
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

  private static func synchronousCaptureOwnsValidImmutablePDFBeforeCommit() throws {
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

  private static func lostStageACKRetriesExactIntentAfterNewerCapture() throws {
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

  private static func prewriteFailureReconcilesBeforeNewCaptureUsesRevision() throws {
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

  private static func loadFailureCannotLetNewerCaptureOvertakeRetainedIngress() throws {
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

  private static func failedNewerIngressBlocksOlderCommitUntilReconciled() throws {
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

  private static func lostCommitACKDoesNotRestageOrCreateAnotherVersion() throws {
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

  private static func recoveredQueuePinsReadsMapsExistingIDsAndCommitsFIFO() throws {
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

  private static func changedCaptureAndWrongRecoveredCopyFailClosed() throws {
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

  private static func corruptRecoveredAssetsRequireActualPDFHashAndSize() throws {
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

  private static func unsafeOrUnboundedLeasesNeverReachStage() throws {
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

  private static func failedDiscardAndForeignCASNeverClearNewerEditor() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let file = try f.lease(PDFJournalFixture.pdf("own", shade: 0.4))
    let foreign = try PDFJournalFixture.pdf("foreign", shade: 0.8)
    let store = PDFJournalStore(canonical: f.base), j = try journal(f, store)
    try j.retain(file); store.failDiscardBeforePersistence()
    refused("Failed discard was silently acknowledged") { try j.discard() }
    expect(store.snapshot().pending.count == 1)
    try j.retain(file)
    store.raceForeignCaptureBeforeDiscard(foreign)
    refused("Discard raced a foreign edit without CAS") { try j.discard() }
    let foreignState = store.snapshot(), attempts = store.discards().count
    expect(foreignState.editorID == "foreign_editor" && foreignState.pending.count == 1)
    refused("Retired editor cleared foreign state on retry") { try j.discard() }
    expect(store.discards().count == attempts && store.snapshot().revision == foreignState.revision)
    let another = try f.lease(f.original)
    refused("Old editor staged over foreign captures") { try j.retain(another) }
    expect(store.snapshot().pending == foreignState.pending && store.versionCount() == 0)
    store.seed(.init(revision: "foreign_empty", editorID: "foreign_editor", origin: f.base, base: f.base, pending: []), assets: [:])
    refused("An old callback cleared a newer editor's empty journal") { try j.discard() }
    expect(store.discards().count == attempts && store.snapshot().revision == "foreign_empty")
  }

  private static func successfulDiscardPermanentlyRetiresTheJournal() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let file = try f.lease(f.original), cap = capture("already_recovered", f.original)
    let store = PDFJournalStore(canonical: f.base)
    store.seed(.init(revision: "before_discard", editorID: "editor", origin: f.base, base: f.base, pending: [cap]), assets: [cap.id: f.original])
    let j = try journal(f, store)
    try j.registerRecovered(file, capture: cap)
    try j.discard(); try j.discard()
    refused("Discarded editor admitted a new capture") { try j.retain(file) }
    refused("Discarded editor attempted a save") { _ = try j.save(file) }
    refused("Discarded editor remapped recovery") { try j.registerRecovered(try f.lease(f.original), capture: cap) }
    refused("Discarded editor read prior private bytes") { _ = try j.recoveredBytes(cap) }
    expect(store.discards().count == 1 && store.stages().isEmpty && store.commits().isEmpty)
    expect(FileManager.default.fileExists(atPath: file.url.path), "Journal discarded a lease it does not own")
    let unwritten = PDFJournalStore(canonical: f.base), failed = try journal(f, unwritten)
    unwritten.failStageBeforePersistence()
    refused("Unwritten capture must remain retryable before discard") { try failed.retain(file) }
    try failed.discard()
    refused("Post-discard reconciliation restaged an old failed capture") { _ = try failed.save(file) }
    expect(unwritten.stages().count == 1 && unwritten.commits().isEmpty)
  }

  private static func emptyJournalStartsFreshWhileRecoveryKeepsOriginAndBase() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let store = PDFJournalStore(canonical: f.base)
    let fresh = try journal(f, store), second = try journal(f, store)
    expect(fresh.origin == f.base && fresh.initialBase == f.base && fresh.editorID != second.editorID)
    let older = PDFBase(version: 3, sha256: String(repeating: "a", count: 64))
    store.seed(.init(revision: "old_completed", editorID: "old_editor", origin: older, base: f.base, pending: []), assets: [:])
    let next = try journal(f, store)
    expect(next.editorID != "old_editor" && next.origin == f.base && next.initialBase == f.base,
                 "An empty completed journal reused a stale editor/origin")
    try next.retain(f.lease(f.original))
    expect(store.snapshot().origin == f.base && store.snapshot().editorID == next.editorID)
  }

  private static func invalidDTOsAndAcknowledgmentsFailClosed() throws {
    let f = try PDFJournalFixture(); defer { f.remove() }
    let store = PDFJournalStore(canonical: f.base), real = store.backend(), cap = capture("valid", f.original)
    let valid = PDFJournal.State(revision: "valid_revision", editorID: "editor", origin: f.base, base: f.base, pending: [cap])
    func state(revision: String = "valid_revision", editor: String? = "editor", origin: PDFBase? = nil,
               base: PDFBase? = nil, pending: [PDFJournal.Capture]? = nil) -> PDFJournal.State {
      .init(revision: revision, editorID: editor, origin: origin ?? f.base, base: base ?? f.base, pending: pending ?? [cap])
    }
    let badStates: [PDFJournal.State] = [
      state(revision: ""), state(revision: String(repeating: "r", count: 129)), state(revision: "bad/revision"),
      state(revision: "révision"), state(revision: "0"), state(editor: "bad editor"),
      .init(revision: "partial", editorID: nil, origin: f.base, base: f.base, pending: []),
      .init(revision: "partial", editorID: "editor", origin: nil, base: f.base, pending: []),
      .init(revision: "partial", editorID: "editor", origin: f.base, base: nil, pending: []),
      .init(revision: "unbound", editorID: nil, origin: nil, base: nil, pending: [cap]),
      state(origin: .init(version: 0, sha256: f.base.sha256)),
      state(base: .init(version: 6, sha256: f.base.sha256)),
      state(base: .init(version: 7, sha256: String(repeating: "b", count: 64))),
      state(origin: .init(version: 7, sha256: String(repeating: "A", count: 64))),
      state(pending: [cap, cap]),
      state(pending: [.init(id: "bad/id", sha256: cap.sha256, size: cap.size)]),
      state(pending: [.init(id: "short", sha256: "bad", size: cap.size)]),
      state(pending: [.init(id: "small", sha256: cap.sha256, size: 4)]),
      state(pending: [.init(id: "huge", sha256: cap.sha256, size: journalMaximum + 1)]),
      state(pending: (0..<17).map { .init(id: "capture_\($0)", sha256: cap.sha256, size: cap.size) }),
      state(pending: (0..<3).map { .init(id: "large_\($0)", sha256: cap.sha256, size: journalMaximum) }),
    ]
    for invalid in badStates {
      let backend = PDFJournal.Backend(load: { invalid }, stage: real.stage, bytes: real.bytes, commit: real.commit, discard: real.discard)
      refused("Malformed DTO admitted a private PDF editor") { _ = try journal(f, store, backend: backend) }
    }
    expect(valid.pending == [cap] && store.stages().isEmpty && store.commits().isEmpty)
    for base in [PDFBase(version: 0, sha256: cap.sha256), .init(version: 1, sha256: "bad")] {
      refused("Invalid observed canonical base admitted") { _ = try PDFJournal(observedBase: base, maximumBytes: journalMaximum, backend: real) }
    }
    for maximum in [UInt64(4), journalMaximum + 1] {
      refused("Invalid configured bounds admitted") { _ = try PDFJournal(observedBase: f.base, maximumBytes: maximum, backend: real) }
    }
    for mode in 0..<3 {
      let model = PDFJournalStore(canonical: f.base), backend = model.backend()
      let wrongDraftACK = PDFJournal.Backend(load: backend.load, stage: { request in
        let actual = try backend.stage(request)
        let captured = actual.draft.pending[0]
        let pending: [PDFJournal.Capture] = mode == 1 ? [] : mode == 2
          ? [.init(id: captured.id, sha256: String(repeating: "a", count: 64), size: captured.size)]
          : actual.draft.pending
        let draft = PDFJournal.State(revision: actual.draft.revision,
          editorID: mode == 0 ? "foreign_editor" : actual.draft.editorID,
          origin: actual.draft.origin, base: actual.draft.base, pending: pending)
        return .init(acceptedRevision: actual.acceptedRevision, draft: draft)
      }, bytes: backend.bytes, commit: backend.commit, discard: backend.discard)
      let j = try journal(f, model, backend: wrongDraftACK), file = try f.lease(f.original)
      refused("Wrong editor/capture ACK admitted publication") { _ = try j.save(file) }
      expect(model.snapshot().pending.count == 1 && model.commits().isEmpty)
    }
    for accepted in ["", "0", "bad/revision", String(repeating: "a", count: 129)] {
      let model = PDFJournalStore(canonical: f.base), backend = model.backend()
      let invalidACK = PDFJournal.Backend(load: backend.load, stage: { request in
        let actual = try backend.stage(request)
        return .init(acceptedRevision: accepted, draft: actual.draft)
      }, bytes: backend.bytes, commit: backend.commit, discard: backend.discard)
      let j = try journal(f, model, backend: invalidACK), file = try f.lease(f.original)
      refused("Malformed stage ACK admitted publication") { try j.retain(file) }
      refused("A save bypassed the malformed stage ACK") { _ = try j.save(file) }
      expect(model.snapshot().pending.count == 1 && model.commits().isEmpty,
                   "Bad ACK rejection must preserve actual persisted bytes, not publish/discard them")
    }
  }
}
