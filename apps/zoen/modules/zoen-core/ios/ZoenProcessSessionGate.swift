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
    let id: UUID
    let session: Session
    let lease: ZoenModuleSessionLease
    let background: Bool
    var ready: Bool
    var quarantined = false
  }
  private struct Selection: Sendable {
    let id: UUID
    let session: Session
    let lease: ZoenModuleSessionLease
  }
  private let close: @Sendable (Session) async throws -> Void
  // A closed foreground entry retains only module affinity. Replacing it retires
  // that module's lease, so a delayed old bridge open cannot reclaim a new owner.
  private var current: Entry?
  private var tail: Task<Void, Never>?
  private var wake: Task<ZoenPushResult, Error>?
  private var wakeID: UUID?
  private var wakeCancellation: ZoenModuleSessionLease?
  private var finishes: [UUID: Int] = [:]
  private var finishWaiters: [UUID: [CheckedContinuation<Void, Never>]] = [:]

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
    if let entry = current {
      let replacing = entry.session !== session || entry.lease !== lease
      if replacing && (entry.ready || entry.background || entry.quarantined || finishes[entry.id, default: 0] > 0) {
        try await close(entry.session)
      }
      // Closing may admit a same-owner erasure escalation. Fence any later
      // admissions synchronously, then drain the already accepted operations.
      if replacing { entry.lease.retire() }
      await drainFinishes(entry.id)
    }
    try Task.checkCancellation()
    guard lease.isLive else { throw ZoenPushGateError.retired }
    let id = UUID()
    current = Entry(id: id, session: session, lease: lease, background: false, ready: false)
    do {
      let value = try await operation()
      guard lease.isLive, current?.id == id else { throw ZoenPushGateError.retired }
      current?.ready = true
      return value
    } catch {
      if current?.id == id { current?.ready = true; current?.quarantined = true }
      throw error
    }
  }

  /// Same-session close/closeForce must reach the actor's existing coalesced
  /// closeTask immediately. Queuing force behind a graceful seal would deadlock
  /// its escalation. An old module's already-drained close is an idempotent no-op.
  func close(session: Session, operation: @escaping @Sendable () async throws -> Void) async throws {
    guard let entry = current, entry.session === session else { return }
    cancelWake()
    try await operation()
    await drainFinishes(entry.id)
    closed(entry.id)
  }

  /// Destructive completion may reopen the existing actor's recovery Client only
  /// while it is still the latest module. It can supersede that actor's graceful
  /// seal; every owner transfer waits for this accepted operation to drain.
  func finishOwned(
    session: Session, lease: ZoenModuleSessionLease,
    operation: @escaping @Sendable () async throws -> Void
  ) async throws {
    guard let entry = current, entry.session === session, entry.lease === lease,
          lease.isLive, !entry.background else { throw ZoenPushGateError.retired }
    cancelWake()
    finishes[entry.id, default: 0] += 1
    current?.ready = true
    defer { finishEnded(entry.id) }
    try await operation()
    if current?.id == entry.id { current?.ready = false }
  }

  private func drainFinishes(_ id: UUID) async {
    if finishes[id, default: 0] > 0 {
      await withCheckedContinuation { finishWaiters[id, default: []].append($0) }
    }
  }

  private func finishEnded(_ id: UUID) {
    let left = finishes[id, default: 1] - 1
    if left > 0 { finishes[id] = left; return }
    finishes.removeValue(forKey: id)
    for waiter in finishWaiters.removeValue(forKey: id) ?? [] { waiter.resume() }
  }

  private func closed(_ id: UUID) {
    guard current?.id == id else { return }
    if current?.lease.isLive == true { current?.ready = false; current?.quarantined = false }
    else { current = nil }
  }

  private func cancelWake() { wakeCancellation?.retire(); wake?.cancel() }
  func becameActive() { cancelWake() }

  /// A duplicate shares no new budget or engine and needs no pending callback queue.
  func runWake(
    make: @escaping @Sendable () -> Session,
    operation: @escaping @Sendable (Session, Bool, ZoenPushWakeLease) async throws -> ZoenPushResult
  ) async -> ZoenPushResult {
    await admitWake(expected: nil, make: make, operation: operation)
  }

  /// A bridge callback borrows only its own already-open module generation. It
  /// cannot choose the process's later account or reopen a closed session.
  func runWake(
    session: Session, lease: ZoenModuleSessionLease,
    operation: @escaping @Sendable (Session, Bool, ZoenPushWakeLease) async throws -> ZoenPushResult
  ) async -> ZoenPushResult {
    guard let entry = current, entry.session === session, entry.lease === lease,
          lease.isLive, entry.ready, !entry.background, !entry.quarantined else { return .noData }
    return await admitWake(expected: Selection(id: entry.id, session: session, lease: lease), make: { session }, operation: operation)
  }

  private func matches(_ expected: Selection) -> Bool {
    current?.id == expected.id && current?.session === expected.session && current?.lease === expected.lease
      && expected.lease.isLive && current?.ready == true && current?.background == false && current?.quarantined == false
  }

  private func admitWake(
    expected: Selection?, make: @escaping @Sendable () -> Session,
    operation: @escaping @Sendable (Session, Bool, ZoenPushWakeLease) async throws -> ZoenPushResult
  ) async -> ZoenPushResult {
    guard wake == nil else { return .noData }
    let id = UUID()
    let admission = ZoenModuleSessionLease()
    let task = Task { try await self.serialized { try await self.wakeNow(expected: expected, make: make, admission: admission, operation: operation) } }
    wakeID = id
    wakeCancellation = admission
    wake = task
    let result = await withTaskCancellationHandler { await task.result } onCancel: { admission.retire(); task.cancel() }
    admission.retire()
    if wakeID == id { wake = nil; wakeID = nil; wakeCancellation = nil }
    switch result { case .success(let value): return value; case .failure: return .failed }
  }

  private func retireCold(_ entry: Entry) async throws {
    if entry.background { entry.lease.retire() }
    do {
      try await close(entry.session)
      await drainFinishes(entry.id)
      closed(entry.id)
    } catch {
      if current?.id == entry.id { current?.quarantined = true }
      throw error
    }
  }

  private func wakeNow(
    expected: Selection?, make: @Sendable () -> Session,
    admission: ZoenModuleSessionLease,
    operation: @Sendable (Session, Bool, ZoenPushWakeLease) async throws -> ZoenPushResult
  ) async throws -> ZoenPushResult {
    try Task.checkCancellation()
    guard admission.isLive else { throw ZoenPushGateError.retired }
    if let expected { guard matches(expected) else { throw ZoenPushGateError.retired } }
    let entry: Entry
    if let active = current {
      // Only an explicit foreground transition can drain a failed retirement.
      guard active.lease.isLive, !active.background, !active.quarantined else { throw ZoenPushGateError.retired }
      entry = active
    } else {
      entry = Entry(id: UUID(), session: make(), lease: ZoenModuleSessionLease(), background: true, ready: false)
      current = entry
    }
    let cold = !entry.ready
    do {
      let value = try await operation(entry.session, cold, ZoenPushWakeLease(parent: entry.lease, work: admission))
      try Task.checkCancellation()
      guard entry.lease.isLive, admission.isLive else { throw ZoenPushGateError.retired }
      if cold { try await retireCold(entry) }
      return value
    } catch {
      if cold, current?.id == entry.id, current?.quarantined != true {
        // Failed close remains the sole quarantined owner. Never silently retry
        // it or construct another Client against the same protected files.
        do { try await retireCold(entry) } catch {}
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
