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
    var retained = false
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
  private var wakePinnedForCall = false
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

  private func cancelWake(preservingCall: Bool = false) {
    if preservingCall && wakePinnedForCall { return }
    wakeCancellation?.retire(); wake?.cancel()
  }
  func becameActive() { cancelWake(preservingCall: true) }

  /// Called only after a canonical incoming invitation has been found, before
  /// system presentation. It preserves the existing OS deadline, not a new budget.
  func preserveWakeForCall(session: Session, lease: ZoenPushWakeLease) -> Bool {
    guard let entry = current, entry.background, entry.session === session,
          entry.lease === lease.parent, lease.isLive, wake != nil,
          wakeCancellation === lease.work else { return false }
    wakePinnedForCall = true
    return true
  }

  /// A foreground module may adopt a cold native call's exact session and lease.
  /// It must not reopen the database or retire the live media port. Other module
  /// replacement and failed-retirement rules remain on the ordinary open path.
  func openAdoptingRetained<T: Sendable>(
    owner: ZoenModuleSessionOwner<Session>, allowed: Bool,
    operation: @escaping @Sendable (Session, ZoenModuleSessionLease, Bool) async throws -> T
  ) async throws -> T {
    cancelWake(preservingCall: allowed)
    return try await serialized {
      let original = owner.current
      guard original.lease.isLive else { throw ZoenPushGateError.retired }
      if allowed, let entry = await self.retainedEntry(),
         owner.adopt(.init(session: entry.session, lease: entry.lease), replacing: original) {
        guard await self.adopted(entry.id) else { throw ZoenPushGateError.retired }
        return try await operation(entry.session, entry.lease, true)
      }
      return try await self.openNow(session: original.session, lease: original.lease) {
        try await operation(original.session, original.lease, false)
      }
    }
  }

  private func retainedEntry() -> Entry? {
    guard let entry = current, entry.background, entry.retained, entry.ready,
          !entry.quarantined, entry.lease.isLive else { return nil }
    return entry
  }
  private func adopted(_ id: UUID) -> Bool {
    guard current?.id == id, current?.lease.isLive == true, current?.retained == true else { return false }
    current = current.map { entry in
      var next = Entry(id: entry.id, session: entry.session, lease: entry.lease, background: false, ready: true)
      next.quarantined = entry.quarantined
      return next
    }
    return true
  }

  /// Completion is ignored after foreground adoption. An old native completion
  /// cannot close a module that now owns the call or a replacement account.
  func releaseRetained(session: Session, lease: ZoenModuleSessionLease) async {
    _ = try? await serialized {
      try await self.releaseRetainedNow(session: session, lease: lease)
    }
  }
  private func releaseRetainedNow(session: Session, lease: ZoenModuleSessionLease) async throws {
    guard let entry = retainedEntry(), entry.session === session, entry.lease === lease else { return }
    try await retireCold(entry)
  }

  /// A duplicate shares no new budget or engine and needs no pending callback queue.
  func runWake(
    make: @escaping @Sendable () -> Session,
    retain: @escaping @Sendable (Session) async -> Bool = { _ in false },
    operation: @escaping @Sendable (Session, Bool, ZoenPushWakeLease) async throws -> ZoenPushResult
  ) async -> ZoenPushResult {
    await admitWake(expected: nil, make: make, retain: retain, operation: operation)
  }

  /// A bridge callback borrows only its own already-open module generation. It
  /// cannot choose the process's later account or reopen a closed session.
  func runWake(
    session: Session, lease: ZoenModuleSessionLease,
    operation: @escaping @Sendable (Session, Bool, ZoenPushWakeLease) async throws -> ZoenPushResult
  ) async -> ZoenPushResult {
    guard let entry = current, entry.session === session, entry.lease === lease,
          lease.isLive, entry.ready, !entry.background, !entry.quarantined else { return .noData }
    return await admitWake(expected: Selection(id: entry.id, session: session, lease: lease), make: { session }, retain: { _ in false }, operation: operation)
  }

  private func matches(_ expected: Selection) -> Bool {
    current?.id == expected.id && current?.session === expected.session && current?.lease === expected.lease
      && expected.lease.isLive && current?.ready == true && current?.background == false && current?.quarantined == false
  }

  private func admitWake(
    expected: Selection?, make: @escaping @Sendable () -> Session,
    retain: @escaping @Sendable (Session) async -> Bool,
    operation: @escaping @Sendable (Session, Bool, ZoenPushWakeLease) async throws -> ZoenPushResult
  ) async -> ZoenPushResult {
    guard wake == nil, current?.retained != true else { return .noData }
    let id = UUID()
    let admission = ZoenModuleSessionLease()
    let task = Task { try await self.serialized { try await self.wakeNow(expected: expected, make: make, admission: admission, retain: retain, operation: operation) } }
    wakeID = id
    wakeCancellation = admission
    wakePinnedForCall = false
    wake = task
    let result = await withTaskCancellationHandler { await task.result } onCancel: { admission.retire(); task.cancel() }
    admission.retire()
    if wakeID == id { wake = nil; wakeID = nil; wakeCancellation = nil; wakePinnedForCall = false }
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
    retain: @Sendable (Session) async -> Bool,
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
      if cold {
        let keep = await retain(entry.session)
        try Task.checkCancellation()
        guard entry.lease.isLive, admission.isLive, current?.id == entry.id else { throw ZoenPushGateError.retired }
        if keep, entry.background { current?.ready = true; current?.retained = true }
        else { try await retireCold(entry) }
      }
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
