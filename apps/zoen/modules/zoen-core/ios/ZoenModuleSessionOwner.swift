import Foundation

/// A module's reference to one process-gated session. This holds no account data.
/// Only the gate may replace an unused module reference with a retained native
/// session; destruction wins a concurrent adoption and retires its exact lease.
final class ZoenModuleSessionOwner<Session: AnyObject & Sendable>: @unchecked Sendable {
  struct Binding: Sendable {
    let session: Session
    let lease: ZoenModuleSessionLease
  }
  private let lock = NSLock()
  private var binding: Binding
  private var retired = false

  init(session: Session) {
    binding = Binding(session: session, lease: ZoenModuleSessionLease())
  }
  var current: Binding {
    lock.lock(); defer { lock.unlock() }
    return binding
  }
  func adopt(_ next: Binding, replacing expected: Binding) -> Bool {
    lock.lock(); defer { lock.unlock() }
    guard !retired, expected.lease.isLive, next.lease.isLive,
          binding.session === expected.session, binding.lease === expected.lease else { return false }
    if expected.lease !== next.lease { expected.lease.retire() }
    binding = next
    return true
  }
  @discardableResult func retire() -> Binding {
    lock.lock(); defer { lock.unlock() }
    retired = true; binding.lease.retire()
    return binding
  }
}
