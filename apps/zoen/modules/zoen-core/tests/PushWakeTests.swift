import Foundation

private func check(_ condition: Bool, _ message: String = "Push contract failed") { precondition(condition, message) }

private actor Entered {
  private var ready = false
  private var waiter: CheckedContinuation<Void, Never>?
  func signal() { ready = true; waiter?.resume(); waiter = nil }
  func wait() async { if !ready { await withCheckedContinuation { waiter = $0 } } }
}

private final class Session: Sendable { let id: Int; init(_ id: Int) { self.id = id } }
private enum Failure: Error { case closed }

private actor Probe {
  private var current: Int?
  private(set) var opens = 0
  private(set) var closes = 0
  private(set) var closeAttempts = 0
  private var failClose = false
  func open(_ id: Int) { check(current == nil, "Concurrent native Client ownership"); current = id; opens += 1 }
  func close(_ id: Int) throws {
    closeAttempts += 1
    if failClose { throw Failure.closed }
    if let active = current { check(active == id); current = nil; closes += 1 }
  }
  func failingClose(_ value: Bool) { failClose = value }
  func count() -> (Int, Int, Int) { (opens, closes, closeAttempts) }
}

private func snapshotModels(_ fixture: URL) throws {
  let data = try Data(contentsOf: fixture)
  let root = try JSONSerialization.jsonObject(with: data) as! [String: Any]
  func encode(_ value: Any) throws -> String { String(data: try JSONSerialization.data(withJSONObject: value), encoding: .utf8)! }
  let before = try ZoenPushSnapshot(json: encode(root["before"]!))
  let after = try ZoenPushSnapshot(json: encode(root["after"]!))
  let notes = after.notes(since: before)
  let expected = root["expected"] as! [[String: String]]
  check(notes.count == expected.count && notes.count == 2)
  for (note, value) in zip(notes, expected) {
    check(note.identifier == value["identifier"] && note.title == value["title"] && note.body == value["body"])
    check(note.spaceID == value["spaceId"] && note.deviceID == value["deviceId"] && note.sequence == value["sequence"])
  }
  var changed = root["after"] as! [String: Any]
  var account = changed["account"] as! [String: Any]
  for field in ["identity_id", "device_id"] {
    var replacement = account; replacement[field] = "other-scope"
    changed["account"] = replacement
    check(try ZoenPushSnapshot(json: encode(changed)).notes(since: before).isEmpty)
  }
  account["unlocked"] = false; changed["account"] = account
  check(try ZoenPushSnapshot(json: encode(changed)).notes(since: before).isEmpty)
  check(after.notes(since: after).isEmpty, "An identical projection cannot create another note")
}

private func payloads() throws {
  func accepts(_ text: String) throws -> Bool {
    ZoenPushWakePayload.accepts(try JSONSerialization.jsonObject(with: Data(text.utf8)) as! [AnyHashable: Any])
  }
  check(try accepts(#"{"aps":{"content-available":1}}"#))
  check(try accepts(#"{"aps":{"content-available":1},"zoen":{"v":1,"space_id":"sp_synthetic","event_seq":"18446744073709551615"}}"#))
  for raw in [
    #"{"aps":{"content-available":true}}"#,
    #"{"aps":{"content-available":1.0}}"#,
    #"{"aps":{"content-available":1,"alert":"UNTRUSTED_CANARY"}}"#,
    #"{"aps":{"content-available":1},"route":"zoen://chat/other"}"#,
    #"{"aps":{"content-available":1},"zoen":{"v":true,"space_id":"sp_test","event_seq":"1"}}"#,
    #"{"aps":{"content-available":1},"zoen":{"v":1,"space_id":"../bad","event_seq":"1"}}"#,
    #"{"aps":{"content-available":1},"zoen":{"v":1,"space_id":"sp_test","event_seq":"18446744073709551616"}}"#,
    #"{"aps":{"content-available":1},"zoen":{"v":1,"space_id":"sp_test","event_seq":1}}"#,
  ] { check(try !accepts(raw)) }
  check(!ZoenPushWakePayload.accepts(["aps": ["content-available": 1], "x": String(repeating: "x", count: 4096)]))
}

private func ownership() async throws {
  let probe = Probe()
  let gate = ZoenProcessSessionGate<Session> { try await probe.close($0.id) }
  let first = Session(1), firstLease = ZoenModuleSessionLease()
  try await gate.open(session: first, lease: firstLease) { await probe.open(1) }
  let borrowed = await gate.runWake(make: { preconditionFailure("Warm wake constructed another session") }) { session, cold, lease in
    check(session === first && !cold && lease.isLive)
    return .newData
  }
  check(borrowed == .newData)
  firstLease.retire()
  var rejected = false
  do { try await gate.open(session: first, lease: firstLease) { () -> Void in preconditionFailure("Retired module reopened") } }
  catch ZoenPushGateError.retired { rejected = true }
  check(rejected, "Retired module admission must fail")
  let second = Session(2)
  try await gate.open(session: second, lease: ZoenModuleSessionLease()) { await probe.open(2) }
  try await gate.close(session: second) { try await probe.close(2) }

  let entered = Entered()
  let work = Task {
    await gate.runWake(make: { Session(3) }) { session, cold, lease in
      check(cold && lease.isLive)
      await probe.open(session.id)
      await entered.signal()
      try await Task.sleep(for: .seconds(30))
      preconditionFailure("Foreground preemption did not cancel the original wake")
    }
  }
  await entered.wait()
  let duplicate = await gate.runWake(make: { preconditionFailure("Duplicate wake created a second session") }) { _, _, _ in preconditionFailure() }
  check(duplicate == .noData)
  let foreground = Session(4)
  try await gate.open(session: foreground, lease: ZoenModuleSessionLease()) { await probe.open(4) }
  check(await work.value == .failed)
  try await gate.close(session: foreground) { try await probe.close(4) }
  let count = await probe.count()
  check(count.0 == 4 && count.1 == 4 && count.2 == 4, "Every opened owner must close exactly once")
}

private func quarantine() async throws {
  let probe = Probe()
  let gate = ZoenProcessSessionGate<Session> { try await probe.close($0.id) }
  await probe.failingClose(true)
  let result = await gate.runWake(make: { Session(1) }) { _, _, _ in await probe.open(1); return .noData }
  check(result == .failed)
  let count = await probe.count(); check(count.2 == 1, "A failed close must not be silently retried")
  let blocked = await gate.runWake(make: { preconditionFailure("Quarantined owner was replaced") }) { _, _, _ in preconditionFailure() }
  check(blocked == .failed)
  await probe.failingClose(false)
  let foreground = Session(2)
  try await gate.open(session: foreground, lease: ZoenModuleSessionLease()) { await probe.open(2) }
  try await gate.close(session: foreground) { try await probe.close(2) }
}

private func expiredWorkAndRetirement() async throws {
  let probe = Probe()
  let gate = ZoenProcessSessionGate<Session> { try await probe.close($0.id) }
  let entered = Entered()
  let work = Task {
    await gate.runWake(make: { Session(1) }) { session, _, _ in
      await probe.open(session.id)
      await entered.signal()
      try await Task.sleep(for: .seconds(30))
      preconditionFailure("Expired work continued")
    }
  }
  await entered.wait()
  work.cancel()
  check(await work.value == .failed)
  let count = await probe.count(); check(count.0 == 1 && count.1 == 1 && count.2 == 1)

  let foreground = Session(2), module = ZoenModuleSessionLease()
  try await gate.open(session: foreground, lease: module) { await probe.open(2) }
  let borrowed = Entered(), release = Entered()
  let late = Task {
    await gate.runWake(make: { preconditionFailure() }) { _, cold, lease in
      check(!cold && lease.isLive)
      await borrowed.signal()
      await release.wait()
      guard lease.isLive else { throw CancellationError() }
      preconditionFailure("A destroyed module admitted a late notification effect")
    }
  }
  await borrowed.wait()
  module.retire()
  await release.signal()
  check(await late.value == .failed)
  try await gate.close(session: foreground) { try await probe.close(2) }
}

private func cancelledQueueCannotOpenAClient() async throws {
  let probe = Probe(), entered = Entered(), release = Entered()
  let gate = ZoenProcessSessionGate<Session> { try await probe.close($0.id) }
  let foreground = Session(1)
  let opening = Task {
    try await gate.open(session: foreground, lease: ZoenModuleSessionLease()) {
      await entered.signal()
      await release.wait()
      await probe.open(1)
    }
  }
  await entered.wait()
  let queued = Task {
    await gate.runWake(make: { preconditionFailure("An expired queued wake opened a session") }) { _, _, _ in preconditionFailure() }
  }
  queued.cancel()
  await release.signal()
  try await opening.value
  check(await queued.value == .failed)
  try await gate.close(session: foreground) { try await probe.close(1) }
  let count = await probe.count(); check(count.0 == 1 && count.1 == 1)
}

@main
private enum PushWakeTests {
  static func main() async throws {
    try payloads()
    try snapshotModels(URL(fileURLWithPath: CommandLine.arguments[1]))
    try await ownership()
    try await quarantine()
    try await expiredWorkAndRetirement()
    try await cancelledQueueCannotOpenAClient()
    let once = ZoenPushCompletion()
    let winners = await withTaskGroup(of: Bool.self) { group in
      for _ in 0..<100 { group.addTask { once.claim() } }
      var count = 0
      for await claimed in group { if claimed { count += 1 } }
      return count
    }
    check(winners == 1 && !once.claim())
    print("PASS native push gate, scope, payload, projection, expiry/preemption and cleanup models")
  }
}
