import Foundation

private actor HandoffEvents {
  var values: [String] = []
  var waiting: CheckedContinuation<String, Never>?
  func emit(_ json: String) {
    if let waiting { self.waiting = nil; waiting.resume(returning: json) }
    else { values.append(json) }
  }
  func nextTicket(action: String = "acquire") async -> String {
    while true {
      let value: String
      if !values.isEmpty { value = values.removeFirst() }
      else { value = await withCheckedContinuation { waiting = $0 } }
      let event = try! JSONSerialization.jsonObject(with: Data(value.utf8)) as! [String: String]
      if event["action"] == action { return event["ticket"]! }
    }
  }
}

@MainActor
private final class PresentationPolicy {
  var blocked = false
  var waiting: CheckedContinuation<Void, Never>?
  func current() async throws -> ZoenCallRecord {
    await withCheckedContinuation { waiting = $0 }
    guard !blocked else { throw ZoenCallFailure.denied }
    return try zoenCallDecode(ZoenCallRecord.self, #"{"call_id":"call_a","invitation":"signed","caller":"alice","caller_device":"a1","callee":"bob","media":"audio","invited_at_ms":1,"expires_at_ms":100,"invitation_confirmed":true,"acceptance_confirmed":false,"invalidated":false}"#)
  }
}

@main
struct CallAudioFocusTests {
  @MainActor
  static func until(_ condition: () -> Bool) async {
    for _ in 0..<10_000 { if condition() { return }; await Task.yield() }
    preconditionFailure("Asynchronous resource completion did not run")
  }
  @MainActor
  static func main() async throws {
    var released = 0
    var completed: [UUID] = []
    let firstOwner = UUID()
    let retiring = ZoenCallRetirement(owner: firstOwner, systemActive: true,
      release: { released += 1 }, completion: { completed.append($0) })
    retiring.systemDidDeactivate() // arrives while RTC stop/terminal action is retained
    precondition(released == 0)
    retiring.mediaDidStop()
    await until { completed.count == 1 }
    precondition(released == 1 && completed == [firstOwner])
    retiring.systemDidReset(); retiring.mediaDidStop(); retiring.systemDidDeactivate()
    precondition(released == 1 && completed.count == 1)

    var heldRelease: CheckedContinuation<Void, Never>?
    var releaseReturned = false
    let secondOwner = UUID()
    let overlapping = ZoenCallRetirement(owner: secondOwner, systemActive: false,
      release: { await withCheckedContinuation { heldRelease = $0 }; releaseReturned = true },
      completion: { completed.append($0) })
    overlapping.mediaDidStop()
    await until { heldRelease != nil }
    overlapping.systemDidActivate() // late activation cannot clear the owner fence
    heldRelease?.resume(); heldRelease = nil
    await until { releaseReturned }
    precondition(completed == [firstOwner])
    overlapping.systemDidDeactivate()
    await until { completed.count == 2 }
    precondition(completed.last == secondOwner)

    let resetOwner = UUID()
    let reset = ZoenCallRetirement(owner: resetOwner, systemActive: true,
      release: { released += 1 }, completion: { completed.append($0) })
    reset.mediaDidStop(); reset.systemDidReset() // no future didDeactivate is required
    await until { completed.count == 3 }
    precondition(released == 2 && completed.last == resetOwner)

    var beforeRTC = ZoenCallMediaIntent()
    beforeRTC.muted = true; beforeRTC.cameraEnabled = false; beforeRTC.speaker = true
    var rtc = beforeRTC // The exact snapshot lent to a newly created native peer.
    precondition(!rtc.microphoneLive(allowed: true, activated: true, live: true))
    precondition(rtc.speaker)
    precondition(!rtc.cameraLive(allowed: true, started: true, live: true))
    rtc.cameraEnabled = true
    precondition(!rtc.cameraLive(allowed: false, started: false, live: true))
    rtc.cameraEnabled = false // user turns it off while permission is pending
    precondition(!rtc.cameraLive(allowed: true, started: false, live: true))
    precondition(!rtc.cameraLive(allowed: true, started: true, live: true)) // late capture completion
    let replacement = ZoenCallMediaIntent()
    precondition(replacement.microphoneLive(allowed: true, activated: true, live: true))
    precondition(!rtc.microphoneLive(allowed: true, activated: true, live: true))
    precondition(!replacement.cameraLive(allowed: true, started: true, live: false))

    // Re-read after the presentation hop; an earlier coordinator snapshot must
    // not admit a peer blocked while that callback was retained.
    let policy = PresentationPolicy()
    let port = ZoenCallPort(scope: .init(identity: "bob", device: "b1", space: "dm", call: "call_a", invitation: "signed"),
      locale: "en", relayReady: { true }, current: { try await policy.current() }, transport: { [] },
      send: { _, _ in }, prepareAudio: {}, releaseAudio: {}, turn: { throw ZoenCallFailure.unavailable }, action: { _ in })
    var reports = 0
    let presenting = Task { @MainActor in
      do { _ = try await port.presentationRecord(); reports += 1 }
      catch ZoenCallFailure.denied { return }
      catch { preconditionFailure("Unexpected presentation failure") }
    }
    await until { policy.waiting != nil }
    policy.blocked = true
    policy.waiting?.resume(); policy.waiting = nil
    await presenting.value
    precondition(reports == 0)

    let focus = ZoenCallAudioFocus()
    let events = HandoffEvents()
    let emit: @Sendable (String) -> Void = { json in Task { await events.emit(json) } }
    try await focus.acquireNative()
    let adoption = Task { try await focus.adopt(emit: emit) }
    let nativeTicket = await events.nextTicket(action: "adopt")
    await focus.answer(ticket: UUID().uuidString, success: true)
    await focus.answer(ticket: nativeTicket, success: true)
    try await adoption.value
    await focus.release()
    let releasedTicket = await events.nextTicket(action: "release")
    precondition(releasedTicket == nativeTicket)

    let first = Task { try await focus.acquire(emit: emit) }
    let old = await events.nextTicket()
    await focus.answer(ticket: UUID().uuidString, success: true)
    do { try await focus.acquire(emit: emit); fatalError("Wrong ticket admitted another owner") }
    catch ZoenCallFailure.unavailable {}
    await focus.answer(ticket: old, success: true)
    try await first.value
    await focus.release()

    let second = Task { try await focus.acquire(emit: emit) }
    let newer = await events.nextTicket()
    precondition(newer != old)
    await focus.answer(ticket: old, success: true)
    await focus.release()
    do { try await second.value; fatalError("Released waiter was admitted") }
    catch ZoenCallFailure.retired {}

    let rejected = Task { try await focus.acquire(emit: emit) }
    let ticket = await events.nextTicket()
    await focus.answer(ticket: ticket, success: false)
    do { try await rejected.value; fatalError("Failed handoff was admitted") }
    catch ZoenCallFailure.unavailable {}
    // Production's real expiry must release the fence, without a fake success.
    do { try await focus.acquire(emit: emit); fatalError("Missing JS acknowledgment never expired") }
    catch ZoenCallFailure.unavailable {}
    let recovered = Task { try await focus.acquire(emit: emit) }
    _ = await events.nextTicket() // the expired ticket
    let final = await events.nextTicket()
    await focus.answer(ticket: final, success: true)
    try await recovered.value
    await focus.release()
    print("PASS actual native call audio handoff, stale tickets, retirement, rejection and expiry")
  }
}
