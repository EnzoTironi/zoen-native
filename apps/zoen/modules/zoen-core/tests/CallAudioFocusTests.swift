import Foundation

private actor HandoffEvents {
  var values: [String] = []
  var waiting: CheckedContinuation<String, Never>?
  func emit(_ json: String) {
    if let waiting { self.waiting = nil; waiting.resume(returning: json) }
    else { values.append(json) }
  }
  func nextAcquire() async -> String {
    while true {
      let value: String
      if !values.isEmpty { value = values.removeFirst() }
      else { value = await withCheckedContinuation { waiting = $0 } }
      let event = try! JSONSerialization.jsonObject(with: Data(value.utf8)) as! [String: String]
      if event["action"] == "acquire" { return event["ticket"]! }
    }
  }
}

@main
struct CallAudioFocusTests {
  static func main() async throws {
    let focus = ZoenCallAudioFocus()
    let events = HandoffEvents()
    let emit: @Sendable (String) -> Void = { json in Task { await events.emit(json) } }
    let first = Task { try await focus.acquire(emit: emit) }
    let old = await events.nextAcquire()
    await focus.answer(ticket: UUID().uuidString, success: true)
    do { try await focus.acquire(emit: emit); fatalError("Wrong ticket admitted another owner") }
    catch ZoenCallFailure.unavailable {}
    await focus.answer(ticket: old, success: true)
    try await first.value
    await focus.release()

    let second = Task { try await focus.acquire(emit: emit) }
    let newer = await events.nextAcquire()
    precondition(newer != old)
    await focus.answer(ticket: old, success: true)
    await focus.release()
    do { try await second.value; fatalError("Released waiter was admitted") }
    catch ZoenCallFailure.retired {}

    let rejected = Task { try await focus.acquire(emit: emit) }
    let ticket = await events.nextAcquire()
    await focus.answer(ticket: ticket, success: false)
    do { try await rejected.value; fatalError("Failed handoff was admitted") }
    catch ZoenCallFailure.unavailable {}
    // Production's real expiry must release the fence, without a fake success.
    do { try await focus.acquire(emit: emit); fatalError("Missing JS acknowledgment never expired") }
    catch ZoenCallFailure.unavailable {}
    let recovered = Task { try await focus.acquire(emit: emit) }
    _ = await events.nextAcquire() // the expired ticket
    let final = await events.nextAcquire()
    await focus.answer(ticket: final, success: true)
    try await recovered.value
    await focus.release()
    print("PASS actual native call audio handoff, stale tickets, retirement, rejection and expiry")
  }
}
