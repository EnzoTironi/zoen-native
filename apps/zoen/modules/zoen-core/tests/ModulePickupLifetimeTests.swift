import Foundation
import Dispatch

private final class Counter: @unchecked Sendable {
  private let lock = NSLock()
  private var count = 0
  func increment() { lock.lock(); count += 1; lock.unlock() }
  var value: Int { lock.lock(); defer { lock.unlock() }; return count }
}

@main
private struct ModulePickupLifetimeTests {
  static func expect(_ condition: @autoclosure () -> Bool, _ message: String) {
    guard condition() else { fatalError(message) }
  }
  static func main() {
    let once = Counter()
    let lifetime = ZoenModulePickupLifetime()
    expect(lifetime.bind(retire: { once.increment() }) != nil, "first Client must bind")
    expect(once.value == 0, "binding must not cancel active work")
    lifetime.retire()
    expect(once.value == 1, "destruction must synchronously revoke its bound Client")
    lifetime.retire()
    expect(once.value == 1, "repeated destruction cannot revoke another owner")

    let rejected = Counter()
    expect(lifetime.bind(retire: { rejected.increment() }) == nil, "destroyed module must reject a suspended open")
    expect(rejected.value == 1, "rejected open must be revoked before bind returns")

    let reusable = ZoenModulePickupLifetime()
    let old = Counter()
    let first = reusable.bind(retire: { old.increment() })!
    reusable.clear(first)
    let fresh = Counter()
    let second = reusable.bind(retire: { fresh.increment() })!
    expect(first != second, "a new Client needs a distinct binding token")
    reusable.clear(first)
    reusable.retire()
    expect(old.value == 0, "already-drained Client must not receive a new cancellation")
    expect(fresh.value == 1, "an old completion cannot clear the replacement Client")

    let duplicate = ZoenModulePickupLifetime()
    let incumbent = Counter()
    let newcomer = Counter()
    expect(duplicate.bind(retire: { incumbent.increment() }) != nil, "incumbent must bind")
    expect(duplicate.bind(retire: { newcomer.increment() }) == nil, "live binding cannot be silently replaced")
    expect(newcomer.value == 1 && incumbent.value == 0, "only the rejected Client must retire")
    duplicate.retire()
    expect(incumbent.value == 1, "incumbent remains owned after duplicate rejection")

    // The native retirement call may acquire another lock. The module guard must
    // release its own condition before calling out, including the rejected path.
    let unlocked = ZoenModulePickupLifetime()
    let completed = DispatchSemaphore(value: 0)
    expect(unlocked.bind(retire: { unlocked.clear(UUID()); completed.signal() }) != nil, "lock-order fixture must bind")
    DispatchQueue.global().async { unlocked.retire() }
    expect(completed.wait(timeout: .now() + 2) == .success, "retirement callback must run outside the module guard lock")
    let lateDone = DispatchSemaphore(value: 0)
    DispatchQueue.global().async {
      let binding = unlocked.bind(retire: { unlocked.clear(UUID()); lateDone.signal() })
      expect(binding == nil, "late callback must not bind")
    }
    expect(lateDone.wait(timeout: .now() + 2) == .success, "rejected Client callback must run outside the guard lock")
    // A second teardown must not return while the first owner is still revoking
    // keys. This exercises two real callers with the native callback held open.
    let concurrent = ZoenModulePickupLifetime()
    let entered = DispatchSemaphore(value: 0)
    let release = DispatchSemaphore(value: 0)
    let firstReturned = DispatchSemaphore(value: 0)
    let secondStarted = DispatchSemaphore(value: 0)
    let secondReturned = DispatchSemaphore(value: 0)
    let calls = Counter()
    expect(concurrent.bind(retire: {
      calls.increment()
      entered.signal()
      expect(release.wait(timeout: .now() + 3) == .success, "test must release the held native callback")
    }) != nil, "concurrent fixture must bind")
    DispatchQueue.global().async { concurrent.retire(); firstReturned.signal() }
    expect(entered.wait(timeout: .now() + 2) == .success, "first teardown must enter native revocation")
    DispatchQueue.global().async {
      secondStarted.signal()
      concurrent.retire()
      secondReturned.signal()
    }
    expect(secondStarted.wait(timeout: .now() + 2) == .success, "second teardown caller must start")
    expect(secondReturned.wait(timeout: .now() + 0.05) == .timedOut, "concurrent teardown cannot return before native revocation finishes")
    release.signal()
    expect(firstReturned.wait(timeout: .now() + 2) == .success, "first teardown must finish after revocation")
    expect(secondReturned.wait(timeout: .now() + 2) == .success, "second teardown must finish after the same revocation")
    expect(calls.value == 1, "concurrent teardown callers must revoke the owner exactly once")
    print("PASS module pickup lifetime: synchronous retirement, terminal late-open refusal, stale-token isolation, duplicate binding, callback lock order and concurrent teardown drain")
  }
}
