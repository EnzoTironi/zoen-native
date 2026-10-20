import Foundation
import ReactNativeEnrichedMarkdown

/// The UIKit sink owns the exact engine and retains bytes before acknowledging input.
/// Actor/MainActor hops and public notifications must stay outside this lock.
final class ZoenPageInputLease: NSObject, ENRMDraftCheckpointSink, @unchecked Sendable {
  enum Failure: LocalizedError {
    case closed, changed, invalidCapture, noCapture, wrongOperation, unavailable, unsupported
    var errorDescription: String? {
      switch self {
      case .closed: NSLocalizedString("This editor has closed. Open the Page again.", comment: "Page input closed")
      case .changed: NSLocalizedString("This Page or its editing account has changed. Open it again.", comment: "Page input owner changed")
      case .invalidCapture: NSLocalizedString("This input could not be saved. Retry or discard it before leaving.", comment: "Page input capture failed")
      case .noCapture: NSLocalizedString("The editor has no retained input to apply.", comment: "Page input absent")
      case .wrongOperation: NSLocalizedString("Retry the pending editor operation before choosing another action.", comment: "Page input mutation pending")
      case .unavailable: NSLocalizedString("The native editor is unavailable. Keep this screen open and retry.", comment: "Page input producer missing")
      case .unsupported: NSLocalizedString("This content requires the Markdown or text editor.", comment: "Page input unsupported")
      }
    }
  }
  enum Operation: String, Sendable { case apply, discard }
  struct Status: Sendable {
    let statusVersion: String
    let valid: Bool
    let everBound: Bool
    let retained: Bool
    let unretained: Bool
    let revision: String
    let sequence: String?
    let failed: Bool
    let sealed: Bool
    let completed: Bool
    let operation: Operation?
  }

  let id: String
  let generation: UInt64
  let scope: FileEditDraftScope
  private let engine: Client
  private let writer: String
  private let lock = NSLock()
  private var statusVersion: UInt64 = 0
  private var live = true
  private var invalidated = false
  private var everBound = false
  private var sealed = false
  private var discardFrozen = false
  private var current: PageInputDto
  private var pending: Mutation?
  private var unretained: Capture?
  private var failure: Error?
  private var completedApply: PageInputApplyResult?
  private var completedDiscard: PageInputAck?
  private let changed: @Sendable (String) -> Void

  private struct Mutation {
    let operation: Operation
    let revision: UInt64
    let sequence: UInt64?
    let sha256: String?
    let id: String
  }
  private struct Capture {
    let revision: UInt64
    let sequence: UInt64
    let payload: Data
  }

  init(id: String, engine: Client, generation: UInt64, scope: FileEditDraftScope,
       opened: PageInputDto, changed: @escaping @Sendable (String) -> Void) throws {
    guard let binding = opened.binding else { throw Failure.changed }
    self.id = id
    self.engine = engine
    self.generation = generation
    self.scope = scope
    self.writer = binding.writerId
    self.current = opened
    self.changed = changed
  }

  /// Completes any callback already holding this lock before fencing all future writes.
  func invalidate() {
    lock.lock(); defer { lock.unlock() }
    statusVersion &+= 1
    live = false
    invalidated = true
    completedApply = nil
    completedDiscard = nil
  }

  func initialSnapshot() throws -> Data? {
    lock.lock(); defer { lock.unlock() }
    guard live, !sealed, !discardFrozen, pending == nil else { throw Failure.closed }
    statusVersion &+= 1
    everBound = true
    return current.capture?.payload
  }

  func initialDraftSnapshotWithError(_ error: AutoreleasingUnsafeMutablePointer<NSError?>?) -> Data? {
    do { return try initialSnapshot() }
    catch let failure { error?.pointee = failure as NSError; return nil }
  }

  func retainDraftSnapshot(_ snapshot: Data, sequence: UInt64) throws -> String {
    lock.lock()
    var notify = false
    defer {
      lock.unlock()
      if notify { changed(id) }
    }
    guard live, !sealed, !discardFrozen, pending == nil else { throw Failure.closed }
    guard !snapshot.isEmpty, snapshot.count <= 1_048_576 else { throw Failure.invalidCapture }
    if let unretained {
      guard unretained.sequence == sequence, unretained.payload == snapshot else {
        throw Failure.invalidCapture
      }
    } else {
      unretained = Capture(revision: current.revision, sequence: sequence, payload: snapshot)
    }
    let wasRetained = current.capture != nil
    let wasFailed = failure != nil
    do {
      let rejected = unretained!
      let ack = try engine.pageInputCheckpoint(scope: scope, writerId: writer,
        expectedRevision: rejected.revision, sequence: rejected.sequence, payload: rejected.payload)
      guard ack.current.binding?.writerId == writer, let capture = ack.current.capture,
            capture.sequence == rejected.sequence, capture.payload == rejected.payload else {
        throw Failure.changed
      }
      statusVersion &+= 1
      current = ack.current
      unretained = nil
      failure = nil
      notify = !wasRetained || wasFailed
      return "\(ack.acceptedRevision):\(capture.sha256)"
    } catch {
      statusVersion &+= 1
      failure = error
      notify = !wasFailed
      throw error
    }
  }

  func draftCheckpointFailed(_ error: Error) {
    lock.lock()
    let notify = live && failure == nil
    statusVersion &+= 1
    failure = error
    lock.unlock()
    if notify { changed(id) }
  }

  /// Called by the main-thread producer owner only after the registry's seal receipt.
  func didSeal() throws {
    lock.lock(); defer { lock.unlock() }
    guard live, !invalidated, !discardFrozen, unretained == nil else { throw Failure.closed }
    statusVersion &+= 1
    sealed = true
  }

  /// Explicit discard closes admission without requiring rejected bytes to become durable.
  func didFreezeForDiscard() throws {
    lock.lock(); defer { lock.unlock() }
    guard live, !invalidated else { throw Failure.closed }
    if let pending, pending.operation != .discard { throw Failure.wrongOperation }
    discardFrozen = true
    statusVersion &+= 1
    sealed = true
  }

  func prepare(_ operation: Operation) throws {
    lock.lock(); defer { lock.unlock() }
    guard !invalidated else { throw Failure.closed }
    if operation == .apply, completedApply != nil { return }
    if operation == .discard, completedDiscard != nil { return }
    guard live else { throw Failure.wrongOperation }
    if let pending {
      guard pending.operation == operation else { throw Failure.wrongOperation }
      return
    }
    guard sealed else { throw Failure.unavailable }
    if operation == .apply {
      guard !discardFrozen, unretained == nil, current.capture != nil else { throw Failure.noCapture }
    } else {
      guard discardFrozen else { throw Failure.unavailable }
    }
    statusVersion &+= 1
    pending = Mutation(operation: operation, revision: current.revision,
      sequence: current.capture?.sequence, sha256: current.capture?.sha256, id: UUID().uuidString)
  }

  func apply() throws -> PageInputApplyResult {
    lock.lock(); defer { lock.unlock() }
    guard !invalidated else { throw Failure.closed }
    if let completedApply { return completedApply }
    guard live, let pending, pending.operation == .apply,
          let sequence = pending.sequence, let sha256 = pending.sha256 else { throw Failure.wrongOperation }
    let result = try engine.pageInputApply(scope: scope, writerId: writer,
      expectedRevision: pending.revision, sequence: sequence, payloadSha256: sha256, mutationId: pending.id)
    statusVersion &+= 1
    current = result.ack.current
    completedApply = result
    failure = nil
    live = false
    return result
  }

  func discard() throws -> PageInputAck {
    lock.lock(); defer { lock.unlock() }
    guard !invalidated else { throw Failure.closed }
    if let completedDiscard { return completedDiscard }
    guard live, let pending, pending.operation == .discard else { throw Failure.wrongOperation }
    let ack: PageInputAck
    if let sequence = pending.sequence, let sha256 = pending.sha256 {
      ack = try engine.pageInputDiscard(scope: scope, writerId: writer,
        expectedRevision: pending.revision, sequence: sequence, payloadSha256: sha256, mutationId: pending.id)
    } else {
      ack = try engine.pageInputReleaseEmpty(scope: scope, writerId: writer,
        expectedRevision: pending.revision, mutationId: pending.id)
    }
    statusVersion &+= 1
    current = ack.current
    completedDiscard = ack
    // The rejected newest snapshot remains owned until this durable tombstone ACK.
    unretained = nil
    failure = nil
    live = false
    return ack
  }

  func releaseEmpty() throws {
    lock.lock(); defer { lock.unlock() }
    guard !invalidated else { throw Failure.closed }
    if completedDiscard != nil { return }
    guard live, !everBound, current.capture == nil, unretained == nil else { throw Failure.wrongOperation }
    if pending == nil {
      pending = Mutation(operation: .discard, revision: current.revision, sequence: nil,
        sha256: nil, id: UUID().uuidString)
    }
    guard let pending, pending.operation == .discard else { throw Failure.wrongOperation }
    // Keep this exact revision/mutation across an ambiguous storage response.
    let ack = try engine.pageInputReleaseEmpty(scope: scope, writerId: writer,
      expectedRevision: pending.revision, mutationId: pending.id)
    statusVersion &+= 1
    current = ack.current
    completedDiscard = ack
    live = false
  }

  func status() -> Status {
    lock.lock(); defer { lock.unlock() }
    return Status(statusVersion: String(statusVersion), valid: !invalidated, everBound: everBound, retained: current.capture != nil,
      unretained: unretained != nil, revision: String(current.revision),
      sequence: current.capture.map { String($0.sequence) }, failed: failure != nil,
      sealed: sealed, completed: completedApply != nil || completedDiscard != nil,
      operation: pending?.operation)
  }
}
