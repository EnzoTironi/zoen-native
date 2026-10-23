import Foundation

private final class Session: Sendable {
  let id: Int
  init(_ id: Int) { self.id = id }
}
private actor Resources {
  var active: Int?
  var opens: [Int] = []
  var closes: [Int] = []
  var lease: ZoenModuleSessionLease?
  func open(_ id: Int) {
    precondition(active == nil, "A second session opened against protected files")
    active = id; opens.append(id)
  }
  func close(_ id: Int) { precondition(active == id); active = nil; closes.append(id) }
  func remember(_ value: ZoenModuleSessionLease) { lease = value }
  func check(active: Int?, opens: [Int], closes: [Int]) {
    precondition(self.active == active && self.opens == opens && self.closes == closes)
  }
}
private actor Latch {
  var started = false
  var continuation: CheckedContinuation<Void, Never>?
  func hold() async { started = true; await withCheckedContinuation { continuation = $0 } }
  func release() { continuation?.resume(); continuation = nil }
  func waitUntilStarted() async {
    for _ in 0..<20_000 { if started { return }; await Task.yield() }
    preconditionFailure("Native work did not reach its handover point")
  }
}

enum RetainedSessionTests {
  static func run() async throws {
    try await foregroundAdoptsPinnedCall()
    try await completedNativeOwnerDoesNotCloseReplacement()
    try await destructionAndModeChangeFenceAdoption()
    try await unpinnedWakeRemainsPreemptible()
    print("PASS retained native session adoption, pinning, destruction and stale completion")
  }

  private static func foregroundAdoptsPinnedCall() async throws {
    let resources = Resources(), latch = Latch()
    let native = Session(1)
    let gate = ZoenProcessSessionGate<Session> { await resources.close($0.id) }
    let wake = Task {
      await gate.runWake(make: { native }, retain: { _ in true }) { session, cold, lease in
        precondition(cold)
        await resources.open(session.id); await resources.remember(lease.parent)
        let pinned = await gate.preserveWakeForCall(session: session, lease: lease)
        precondition(pinned)
        await latch.hold()
        precondition(lease.isLive, "Foreground activation cancelled a reported call")
        return .newData
      }
    }
    await latch.waitUntilStarted()
    let owner = ZoenModuleSessionOwner(session: Session(2))
    let unused = owner.current
    await gate.becameActive()
    let foreground = Task {
      try await gate.openAdoptingRetained(owner: owner, allowed: true) { session, lease, adopted in
        precondition(adopted && session === native && lease.isLive)
        return session.id
      }
    }
    await latch.release()
    let result = await wake.value
    let adoptedID = try await foreground.value
    precondition(result == .newData && adoptedID == 1)
    precondition(!unused.lease.isLive && owner.current.session === native)
    await resources.check(active: 1, opens: [1], closes: [])
    let lease = owner.current.lease
    await gate.releaseRetained(session: native, lease: lease) // queued native end after adoption
    await resources.check(active: 1, opens: [1], closes: [])
    try await gate.close(session: native) { await resources.close(1) }
    await resources.check(active: nil, opens: [1], closes: [1])
  }

  private static func completedNativeOwnerDoesNotCloseReplacement() async throws {
    let resources = Resources(), native = Session(1)
    let gate = ZoenProcessSessionGate<Session> { await resources.close($0.id) }
    _ = await gate.runWake(make: { native }, retain: { _ in true }) { _, _, lease in
      await resources.open(1); await resources.remember(lease.parent); return .newData
    }
    let lease = await resources.lease!
    await gate.releaseRetained(session: native, lease: lease)
    precondition(!lease.isLive)
    let owner = ZoenModuleSessionOwner(session: Session(2))
    try await gate.openAdoptingRetained(owner: owner, allowed: true) { session, _, adopted in
      precondition(!adopted && session.id == 2); await resources.open(2)
    }
    await gate.releaseRetained(session: native, lease: lease)
    await resources.check(active: 2, opens: [1, 2], closes: [1])
    let current = owner.current
    try await gate.close(session: current.session) { await resources.close(2) }
  }

  private static func destructionAndModeChangeFenceAdoption() async throws {
    let resources = Resources(), native = Session(1)
    let gate = ZoenProcessSessionGate<Session> { await resources.close($0.id) }
    _ = await gate.runWake(make: { native }, retain: { _ in true }) { _, _, lease in
      await resources.open(1); await resources.remember(lease.parent); return .newData
    }
    let retired = ZoenModuleSessionOwner(session: Session(2))
    let original = retired.current
    retired.retire()
    let nativeLease = await resources.lease!
    precondition(!retired.adopt(.init(session: native, lease: nativeLease), replacing: original))
    do {
      try await gate.openAdoptingRetained(owner: retired, allowed: true) { _, _, _ -> Void in
        preconditionFailure("A destroyed module adopted protected resources")
      }
      preconditionFailure("Retired owner was admitted")
    } catch ZoenPushGateError.retired {}
    await resources.check(active: 1, opens: [1], closes: [])
    let demo = ZoenModuleSessionOwner(session: Session(3))
    try await gate.openAdoptingRetained(owner: demo, allowed: false) { session, _, adopted in
      precondition(!adopted); await resources.open(session.id)
    }
    precondition(!nativeLease.isLive)
    await resources.check(active: 3, opens: [1, 3], closes: [1])
    try await gate.close(session: demo.current.session) { await resources.close(3) }
  }

  private static func unpinnedWakeRemainsPreemptible() async throws {
    let resources = Resources(), latch = Latch()
    let gate = ZoenProcessSessionGate<Session> { await resources.close($0.id) }
    let wake = Task {
      await gate.runWake(make: { Session(1) }, retain: { _ in true }) { session, _, lease in
        await resources.open(1)
        let forged = ZoenPushWakeLease(parent: lease.parent, work: ZoenModuleSessionLease())
        let accepted = await gate.preserveWakeForCall(session: session, lease: forged)
        precondition(!accepted)
        await latch.hold()
        return .noData
      }
    }
    await latch.waitUntilStarted()
    let owner = ZoenModuleSessionOwner(session: Session(2))
    let foreground = Task {
      try await gate.openAdoptingRetained(owner: owner, allowed: true) { session, _, adopted in
        precondition(!adopted); await resources.open(session.id)
      }
    }
    await gate.becameActive()
    await latch.release()
    let result = await wake.value
    try await foreground.value
    precondition(result == .failed)
    await resources.check(active: 2, opens: [1, 2], closes: [1])
    try await gate.close(session: owner.current.session) { await resources.close(2) }
  }
}
