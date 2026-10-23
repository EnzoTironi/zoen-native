import Foundation

/// Ephemeral module admission, not an account, vault or journal.
final class ZoenModuleSessionLease: @unchecked Sendable {
  private let lock = NSLock()
  private var live = true
  var isLive: Bool { lock.lock(); defer { lock.unlock() }; return live }
  func retire() { lock.lock(); live = false; lock.unlock() }
}

enum ZoenPushResult: Equatable, Sendable { case newData, noData, failed }
enum ZoenPushGateError: Error { case retired }
struct ZoenPushWakeLease: Sendable {
  let parent: ZoenModuleSessionLease
  let work: ZoenModuleSessionLease
  var isLive: Bool { parent.isLive && work.isLive }
}

/// One process-wide transition queue. Borrowers never construct a second Client.
/// Sessions remain owned by their original module, so late bridge callbacks cannot
/// gain access to a later module's account. Only cold work receives a new session.
actor ZoenProcessSessionGate<Session: AnyObject & Sendable> {
  private struct Entry: Sendable {
    let session: Session
    let lease: ZoenModuleSessionLease
    let background: Bool
  }
  private let close: @Sendable (Session) async throws -> Void
  private var current: Entry?
  private var tail: Task<Void, Never>?
  private var wake: Task<ZoenPushResult, Error>?
  private var wakeID: UUID?
  private var wakeCancellation: ZoenModuleSessionLease?

  init(close: @escaping @Sendable (Session) async throws -> Void) { self.close = close }

  private func serialized<T: Sendable>(
    _ operation: @escaping @Sendable () async throws -> T
  ) async throws -> T {
    let previous = tail
    let task = Task {
      await previous?.value
      try Task.checkCancellation()
      return try await operation()
    }
    tail = Task { _ = await task.result }
    return try await withTaskCancellationHandler { try await task.value } onCancel: { task.cancel() }
  }

  func open<T: Sendable>(
    session: Session, lease: ZoenModuleSessionLease,
    operation: @escaping @Sendable () async throws -> T
  ) async throws -> T {
    guard lease.isLive else { throw ZoenPushGateError.retired }
    cancelWake()
    return try await serialized { try await self.openNow(session: session, lease: lease, operation: operation) }
  }

  private func openNow<T: Sendable>(
    session: Session, lease: ZoenModuleSessionLease,
    operation: @escaping @Sendable () async throws -> T
  ) async throws -> T {
    guard lease.isLive else { throw ZoenPushGateError.retired }
    if let entry = current, entry.session !== session {
      try await close(entry.session)
      current = nil
    }
    try Task.checkCancellation()
    guard lease.isLive else { throw ZoenPushGateError.retired }
    current = Entry(session: session, lease: lease, background: false)
    return try await operation()
  }

  func close<T: Sendable>(session: Session, operation: @escaping @Sendable () async throws -> T) async throws -> T {
    if current?.session === session { cancelWake() }
    return try await serialized {
      let result = try await operation()
      await self.closed(session)
      return result
    }
  }

  private func closed(_ session: Session) {
    if current?.session === session { current = nil }
  }

  private func cancelWake() { wakeCancellation?.retire(); wake?.cancel() }
  func becameActive() { cancelWake() }

  /// A duplicate shares no new budget or engine and needs no pending callback queue.
  func runWake(
    make: @escaping @Sendable () -> Session,
    operation: @escaping @Sendable (Session, Bool, ZoenPushWakeLease) async throws -> ZoenPushResult
  ) async -> ZoenPushResult {
    guard wake == nil else { return .noData }
    let id = UUID()
    let admission = ZoenModuleSessionLease()
    let task = Task { try await self.serialized { try await self.wakeNow(make: make, admission: admission, operation: operation) } }
    wakeID = id
    wakeCancellation = admission
    wake = task
    let result = await withTaskCancellationHandler { await task.result } onCancel: { admission.retire(); task.cancel() }
    admission.retire()
    if wakeID == id { wake = nil; wakeID = nil; wakeCancellation = nil }
    switch result { case .success(let value): return value; case .failure: return .failed }
  }

  private func wakeNow(
    make: @Sendable () -> Session,
    admission: ZoenModuleSessionLease,
    operation: @Sendable (Session, Bool, ZoenPushWakeLease) async throws -> ZoenPushResult
  ) async throws -> ZoenPushResult {
    try Task.checkCancellation()
    guard admission.isLive else { throw ZoenPushGateError.retired }
    let cold = current == nil
    let entry: Entry
    if let active = current {
      // A failed retirement remains quarantined. Never repair it by opening
      // another engine against the same files from a notification callback.
      guard active.lease.isLive, !active.background else { throw ZoenPushGateError.retired }
      entry = active
    } else {
      entry = Entry(session: make(), lease: ZoenModuleSessionLease(), background: true)
      current = entry
    }
    do {
      let value = try await operation(entry.session, cold, ZoenPushWakeLease(parent: entry.lease, work: admission))
      try Task.checkCancellation()
      guard entry.lease.isLive else { throw ZoenPushGateError.retired }
      if cold { entry.lease.retire(); try await close(entry.session); current = nil }
      return value
    } catch {
      if cold && entry.lease.isLive {
        entry.lease.retire()
        // Keep a failed close as the sole quarantined owner; do not create a
        // replacement session until an explicit foreground transition drains it.
        do { try await close(entry.session); current = nil } catch {}
      }
      throw error
    }
  }
}

/// Complete each OS callback once, including expiry racing normal completion.
final class ZoenPushCompletion: @unchecked Sendable {
  private let lock = NSLock()
  private var finished = false
  func claim() -> Bool {
    lock.lock(); defer { lock.unlock() }
    guard !finished else { return false }
    finished = true
    return true
  }
}
