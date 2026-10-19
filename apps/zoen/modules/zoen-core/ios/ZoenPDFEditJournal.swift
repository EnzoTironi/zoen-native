import CryptoKit
import Foundation
import PDFKit

/// The native editor keeps only disposable file leases. This adapter admits each
/// complete Quick Look capture to the encrypted Rust journal before publishing it.
/// One lock orders synchronous capture against an asynchronous worker's commit;
/// there is no suspension while it is held and Rust remains the durable owner.
final class ZoenPDFEditJournal: @unchecked Sendable {
  struct Capture: Sendable, Equatable {
    let id: String
    let sha256: String
    let size: UInt64
  }

  struct State: Sendable {
    let revision: String
    let editorID: String?
    let origin: ZoenFilePreview.EditBase?
    let base: ZoenFilePreview.EditBase?
    let pending: [Capture]
  }

  struct Stage: Sendable {
    let expectedRevision: String
    let editorID: String
    let captureID: String
    let origin: ZoenFilePreview.EditBase
    let bytes: Data
  }

  struct Acknowledgment: Sendable {
    let acceptedRevision: String
    let draft: State
  }

  struct Backend: Sendable {
    let load: @Sendable () throws -> State
    let stage: @Sendable (Stage) throws -> Acknowledgment
    let bytes: @Sendable (String, String) throws -> Data
    let commit: @Sendable (String, String) throws -> ZoenFilePreview.EditBase
    let discard: @Sendable (String) throws -> State
  }

  private struct Attempt {
    let file: ZoenPreviewFile
    let captureID: String
    var capture: Capture?
    // Keep the original expected revision even if a native acknowledgment was
    // lost and later captures have already advanced the private journal.
    var expectedRevision: String?
    var acknowledged: Bool
  }

  private let lock = NSLock()
  private let backend: Backend
  private let maximumBytes: UInt64
  let recovered: State
  let origin: ZoenFilePreview.EditBase
  let editorID: String
  private var attempts: [URL: Attempt] = [:]
  private var attemptOrder: [URL] = []
  private var discarded = false

  init(observedBase: ZoenFilePreview.EditBase, maximumBytes: UInt64, backend: Backend) throws {
    guard observedBase.version > 0, Self.validSHA(observedBase.sha256),
          maximumBytes >= 5, maximumBytes <= 64 << 20 else { throw ZoenFilePreview.Failure.changed }
    self.backend = backend
    self.maximumBytes = maximumBytes
    let state = try backend.load()
    try Self.validate(state, maximumBytes: maximumBytes)
    recovered = state
    if !state.pending.isEmpty {
      guard let editorID = state.editorID, let origin = state.origin else { throw ZoenFilePreview.Failure.changed }
      self.editorID = editorID
      self.origin = origin
    } else {
      editorID = UUID().uuidString
      origin = observedBase
    }
  }

  /// Original current version, or the journal's exact checked base after any
  /// earlier accepted captures. It is never rebased over another writer's edit.
  var initialBase: ZoenFilePreview.EditBase { recovered.pending.isEmpty ? origin : recovered.base! }

  func recoveredBytes(_ capture: Capture) throws -> Data {
    lock.lock(); defer { lock.unlock() }
    guard !discarded, recovered.pending.contains(capture) else { throw ZoenFilePreview.Failure.changed }
    let bytes = try backend.bytes(recovered.revision, capture.id)
    try Self.validate(bytes, capture: capture, maximumBytes: maximumBytes)
    return bytes
  }

  /// The caller has just materialized verified private bytes into a fresh owned
  /// lease. Map that lease back to its existing immutable capture, not a new edit.
  func registerRecovered(_ file: ZoenPreviewFile, capture: Capture) throws {
    lock.lock(); defer { lock.unlock() }
    guard !discarded, recovered.pending.contains(capture), attempts[file.url] == nil else {
      throw ZoenFilePreview.Failure.changed
    }
    let bytes = try Self.read(file, maximumBytes: maximumBytes)
    try Self.validate(bytes, capture: capture, maximumBytes: maximumBytes)
    attempts[file.url] = Attempt(file: file, captureID: capture.id, capture: capture,
      expectedRevision: recovered.revision, acknowledged: true)
    attemptOrder.append(file.url)
  }

  func retain(_ file: ZoenPreviewFile) throws {
    lock.lock(); defer { lock.unlock() }
    try reserve(file)
    try reconcileUnacknowledged()
    try retainLocked(file)
  }

  private func reserve(_ file: ZoenPreviewFile) throws {
    guard !discarded else { throw ZoenFilePreview.Failure.changed }
    if let previous = attempts[file.url] {
      guard previous.file.directory == file.directory else { throw ZoenFilePreview.Failure.changed }
      return
    }
    // Reserve FIFO ownership before even a file read or metadata load can fail.
    // A later callback must never overtake a copy waiting at those boundaries.
    attempts[file.url] = Attempt(file: file, captureID: UUID().uuidString, capture: nil,
      expectedRevision: nil, acknowledged: false)
    attemptOrder.append(file.url)
  }

  private func reconcileUnacknowledged() throws {
    // An unresolved ingress may have failed either before storage or after its
    // ACK was lost. Retry the exact intent before another stage/commit can move
    // its expected revision. Never guess which side of that boundary won.
    for url in attemptOrder {
      if let attempt = attempts[url], !attempt.acknowledged { try retainLocked(attempt.file) }
    }
  }

  private func retainLocked(_ file: ZoenPreviewFile) throws {
    guard !discarded else { throw ZoenFilePreview.Failure.changed }
    let bytes = try Self.read(file, maximumBytes: maximumBytes)
    guard var attempt = attempts[file.url] else { throw ZoenFilePreview.Failure.changed }
    if let capture = attempt.capture {
      try Self.validate(bytes, capture: capture, maximumBytes: maximumBytes)
    } else {
      attempt.capture = Capture(id: attempt.captureID,
        sha256: SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined(), size: UInt64(bytes.count))
      attempts[file.url] = attempt
    }
    if attempt.acknowledged { return }
    if attempt.expectedRevision == nil {
      let state = try backend.load()
      try Self.validate(state, maximumBytes: maximumBytes)
      guard state.pending.isEmpty || state.editorID == editorID else { throw ZoenFilePreview.Failure.changed }
      attempt.expectedRevision = state.revision
      attempts[file.url] = attempt
    }
    guard let expectedRevision = attempt.expectedRevision, let capture = attempt.capture else {
      throw ZoenFilePreview.Failure.changed
    }
    let ack = try backend.stage(Stage(expectedRevision: expectedRevision, editorID: editorID,
      captureID: capture.id, origin: origin, bytes: bytes))
    try Self.validate(ack.draft, maximumBytes: maximumBytes)
    guard Self.validToken(ack.acceptedRevision), ack.acceptedRevision != "0",
          ack.draft.editorID == editorID, ack.draft.origin == origin,
          ack.draft.pending.contains(capture) else { throw ZoenFilePreview.Failure.changed }
    attempts[file.url]?.acknowledged = true
  }

  func save(_ file: ZoenPreviewFile) throws -> ZoenFilePreview.EditBase {
    lock.lock(); defer { lock.unlock() }
    try reserve(file)
    // A newer capture can fail private ingress while this older save is queued
    // on the session actor. It must be reconciled before this commit advances CAS.
    try reconcileUnacknowledged()
    try retainLocked(file)
    guard let attempt = attempts[file.url], attempt.acknowledged, let capture = attempt.capture else { throw ZoenFilePreview.Failure.changed }
    let accepted = try backend.commit(editorID, capture.id)
    guard accepted.version > 0, Self.validSHA(accepted.sha256) else { throw ZoenFilePreview.Failure.changed }
    return accepted
  }

  func discard() throws {
    lock.lock(); defer { lock.unlock() }
    if discarded { return }
    let current = try backend.load()
    try Self.validate(current, maximumBytes: maximumBytes)
    // Never clear another editor's newer captures, including after a delayed
    // callback or a stale recovery view. The core independently enforces CAS.
    guard current.editorID == nil || current.editorID == editorID else { throw ZoenFilePreview.Failure.changed }
    let after = try backend.discard(current.revision)
    try Self.validate(after, maximumBytes: maximumBytes)
    guard after.pending.isEmpty else { throw ZoenFilePreview.Failure.changed }
    discarded = true
  }

  private static func validSHA(_ value: String) -> Bool {
    value.utf8.count == 64 && value.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }
  }

  private static func validToken(_ value: String) -> Bool {
    !value.isEmpty && value.utf8.count <= 128 && value.utf8.allSatisfy {
      (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) || $0 == 95 || $0 == 45
    }
  }

  private static func validate(_ state: State, maximumBytes: UInt64) throws {
    guard Self.validToken(state.revision), state.pending.count <= 16,
          Set(state.pending.map(\.id)).count == state.pending.count,
          state.pending.allSatisfy({ Self.validToken($0.id) && Self.validSHA($0.sha256)
            && $0.size >= 5 && $0.size <= maximumBytes }) else { throw ZoenFilePreview.Failure.changed }
    var total: UInt64 = 0
    for capture in state.pending {
      let next = total.addingReportingOverflow(capture.size)
      guard !next.overflow, next.partialValue <= 128 << 20 else { throw ZoenFilePreview.Failure.changed }
      total = next.partialValue
    }
    if state.editorID != nil || state.origin != nil || state.base != nil {
      guard let editor = state.editorID, Self.validToken(editor),
            let origin = state.origin, origin.version > 0, Self.validSHA(origin.sha256),
            let base = state.base, base.version >= origin.version, Self.validSHA(base.sha256),
            base.version != origin.version || base.sha256 == origin.sha256 else { throw ZoenFilePreview.Failure.changed }
    } else if !state.pending.isEmpty { throw ZoenFilePreview.Failure.changed }
    if state.revision == "0", state.editorID != nil || !state.pending.isEmpty {
      throw ZoenFilePreview.Failure.changed
    }
  }

  private static func validate(_ bytes: Data, capture: Capture, maximumBytes: UInt64) throws {
    try validatePDF(bytes, maximumBytes: maximumBytes)
    guard UInt64(bytes.count) == capture.size,
          SHA256.hash(data: bytes).map({ String(format: "%02x", $0) }).joined() == capture.sha256 else {
      throw ZoenFilePreview.Failure.editedPDF
    }
  }

  static func read(_ file: ZoenPreviewFile, maximumBytes: UInt64) throws -> Data {
    let manager = FileManager.default
    guard file.url.isFileURL, file.directory.isFileURL,
          file.url.deletingLastPathComponent().standardizedFileURL == file.directory.standardizedFileURL,
          try manager.attributesOfItem(atPath: file.directory.path)[.type] as? FileAttributeType == .typeDirectory else {
      throw ZoenFilePreview.Failure.editedPDF
    }
    let attributes = try manager.attributesOfItem(atPath: file.url.path)
    guard attributes[.type] as? FileAttributeType == .typeRegular,
          let size = attributes[.size] as? NSNumber, size.uint64Value >= 5,
          size.uint64Value <= maximumBytes else { throw ZoenFilePreview.Failure.editedPDF }
    let bytes = try Data(contentsOf: file.url, options: [.mappedIfSafe])
    guard UInt64(bytes.count) == size.uint64Value else { throw ZoenFilePreview.Failure.editedPDF }
    try validatePDF(bytes, maximumBytes: maximumBytes)
    return bytes
  }

  private static func validatePDF(_ bytes: Data, maximumBytes: UInt64) throws {
    guard UInt64(bytes.count) <= maximumBytes, bytes.starts(with: Data("%PDF-".utf8)),
          let document = PDFDocument(data: bytes), !document.isLocked, document.pageCount > 0 else {
      throw ZoenFilePreview.Failure.editedPDF
    }
  }
}
