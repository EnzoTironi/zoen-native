import Foundation

/// Module destruction revokes pickup synchronously, before its actor teardown is scheduled.
/// The closure owns no account data; it only retires the exact bound native Client.
final class ZoenModulePickupLifetime: @unchecked Sendable {
  private struct Binding {
    let id: UUID
    let retire: @Sendable () -> Void
  }
  private let condition = NSCondition()
  private var terminal = false
  private var retiring = false
  private var binding: Binding?

  func bind(retire: @escaping @Sendable () -> Void) -> UUID? {
    condition.lock()
    guard !terminal, binding == nil else {
      condition.unlock()
      // A suspended open cannot publish a Client after its module was destroyed.
      retire()
      return nil
    }
    let id = UUID()
    binding = Binding(id: id, retire: retire)
    condition.unlock()
    return id
  }

  /// Ordinary close clears only its own binding after real pickup/request drain.
  func clear(_ id: UUID) {
    condition.lock()
    if binding?.id == id { binding = nil }
    condition.unlock()
  }

  func retire() {
    condition.lock()
    terminal = true
    while retiring { condition.wait() }
    let action = binding?.retire
    binding = nil
    retiring = action != nil
    condition.unlock()

    // Never call Rust (or acquire its install gate) while holding this lock.
    // This synchronous closure must not reenter retire() on this same lifetime.
    action?()

    condition.lock()
    retiring = false
    condition.broadcast()
    condition.unlock()
  }
}
