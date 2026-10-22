import CoreGraphics
import CryptoKit
import Foundation
import PDFKit

typealias PDFJournal = ZoenPDFEditJournal
typealias PDFBase = ZoenFilePreview.EditBase
let journalMaximum: UInt64 = 64 << 20

func pdfDigest(_ bytes: Data) -> String {
  SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
}

/// Synchronous encrypted-core port model: receipts are exact intents, commits
/// consume only the FIFO head, and read/discard require the observed revision.
/// It stores a physical copy of ingress bytes, as Rust does before returning.
final class PDFJournalStore: @unchecked Sendable {
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

final class PDFJournalFixture {
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
