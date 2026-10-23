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
private actor Effects {
  private(set) var count = 0
  func record() { count += 1 }
}

private actor Seal {
  let entered = Entered()
  private let release = Entered()
  private var task: Task<Void, Never>?
  private(set) var starts = 0
  private(set) var escalations = 0
  func close(force: Bool = false) async {
    if force { escalations += 1; await release.signal() }
    if let task { await task.value; return }
    starts += 1
    let pending = Task { await entered.signal(); await release.wait() }
    task = pending
    await pending.value
  }
}

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

private func fetchedEvents(_ fixture: URL) throws {
  let root = try JSONSerialization.jsonObject(with: Data(contentsOf: fixture)) as! [String: Any]
  let seed = root["after"] as! [String: Any]
  func model(_ count: String, chat: [String: Any] = [:], account: [String: Any] = [:], synced: Bool = true) throws -> ZoenPushSnapshot {
    var value = seed
    var item = (seed["conversations"] as! [[String: Any]])[0]
    item["event_count"] = count
    for (key, field) in chat { item[key] = field }
    var owner = seed["account"] as! [String: Any]
    for (key, field) in account { owner[key] = field }
    value["conversations"] = [item]
    value["account"] = owner
    value["connection"] = ["synced": synced]
    let data = try JSONSerialization.data(withJSONObject: value)
    return try ZoenPushSnapshot(json: String(decoding: data, as: UTF8.self))
  }
  let before = try model("1")
  for (old, new, expected) in [("0", "1", true), ("1", "2", true), ("1", "1", false), ("2", "1", false),
                              ("18446744073709551614", "18446744073709551615", true)] {
    check(try model(new).hasFetchedEvents(since: model(old)) == expected)
  }
  check(try model("1", chat: ["id": "new"]).hasFetchedEvents(since: before))
  check(try !model("0", chat: ["id": "new"]).hasFetchedEvents(since: before))
  for invalid in ["", "00", "01", "-1", "1.0", "18446744073709551616"] {
    check(try !model(invalid).hasFetchedEvents(since: before))
    check(try !model("2").hasFetchedEvents(since: model(invalid)))
  }
  for key in ["muted", "archived"] {
    let quiet = try model("2", chat: [key: true])
    check(quiet.hasFetchedEvents(since: before) && quiet.notes(since: before).isEmpty)
  }
  let permissionIndependent = try model("2")
  let unreadBaseline = try model("1", chat: ["unread": 0])
  check(permissionIndependent.notes(since: unreadBaseline).count == 1)
  check(permissionIndependent.hasFetchedEvents(since: unreadBaseline), "Fetch progress does not require presentation permission")
  for key in ["identity_id", "device_id"] {
    check(try !model("2", account: [key: "other-owner"]).hasFetchedEvents(since: before))
  }
  check(try !model("2", account: ["unlocked": false]).hasFetchedEvents(since: before))
  check(try !model("2", account: ["setup_pending": true]).hasFetchedEvents(since: before))
  check(try !model("2", synced: false).hasFetchedEvents(since: before))
  check(try model("2").hasFetchedEvents(since: model("1", synced: false)))
}

private func payloads() throws {
  let info: [String: Any] = ["ZoenRemoteNotificationsEnabled": true, "ZoenPushEnvironment": "sandbox", "ZoenPushTopic": "xyz.tironi.zoen"]
  check(ZoenPushBuildAdmission.enabled(info, bundleIdentifier: "xyz.tironi.zoen"))
  check(!ZoenPushBuildAdmission.enabled(info, bundleIdentifier: nil))
  check(!ZoenPushBuildAdmission.enabled(info, bundleIdentifier: "other.app"))
  for (field, value) in [("ZoenRemoteNotificationsEnabled", false as Any), ("ZoenPushEnvironment", "debug" as Any), ("ZoenPushTopic", "other.app" as Any)] {
    var changed = info; changed[field] = value
    check(!ZoenPushBuildAdmission.enabled(changed, bundleIdentifier: "xyz.tironi.zoen"))
  }
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

private func staleCloseCannotReenterAnOlderSession() async throws {
  let probe = Probe(), effects = Effects()
  let gate = ZoenProcessSessionGate<Session> { try await probe.close($0.id) }
  let first = Session(1), second = Session(2), oldLease = ZoenModuleSessionLease()
  try await gate.open(session: first, lease: oldLease) { await probe.open(1) }
  try await gate.open(session: second, lease: ZoenModuleSessionLease()) { await probe.open(2) }
  // Old OnDestroy/close callbacks can arrive after replacement. Their callback
  // must not retire the new module's UI or reopen an erasure-recovery Client.
  try await gate.close(session: first) { await effects.record() }
  check(await effects.count == 0, "A stale module callback reentered an already retired session")
  check(!oldLease.isLive, "Replacement must retire the previous module's lease")
  var rejected = false
  do { try await gate.open(session: first, lease: oldLease) { await effects.record() } }
  catch ZoenPushGateError.retired { rejected = true }
  let oldOpenEffects = await effects.count
  check(rejected && oldOpenEffects == 0, "An old bridge open reclaimed the new owner")
  rejected = false
  do { try await gate.finishOwned(session: first, lease: oldLease) { await effects.record() } }
  catch ZoenPushGateError.retired { rejected = true }
  let oldEraseEffects = await effects.count
  check(rejected && oldEraseEffects == 0, "Old erasure reopened a recovery Client")
  try await gate.close(session: second) { try await probe.close(2) }
  let count = await probe.count()
  check(count.0 == 2 && count.1 == 2 && count.2 == 2)
}

private func forceAndErasureCanEscalateAGracefulSeal() async throws {
  for erasure in [false, true] {
    let session = Seal(), lease = ZoenModuleSessionLease()
    let gate = ZoenProcessSessionGate<Seal> { await $0.close(force: true) }
    try await gate.open(session: session, lease: lease) {}
    let graceful = Task { try await gate.close(session: session) { await session.close() } }
    await session.entered.wait()
    if erasure {
      try await gate.finishOwned(session: session, lease: lease) { await session.close(force: true) }
    } else {
      try await gate.close(session: session) { await session.close(force: true) }
    }
    try await graceful.value
    check(await session.starts == 1, "Escalation must borrow the actor's existing closeTask")
    check(await session.escalations == 1)
  }
}

private func transferDrainsLatestModuleRecoveryAfterClose() async throws {
  let probe = Probe(), started = Entered(), release = Entered(), closed = Entered(), effects = Effects()
  let first = Session(1), second = Session(2), lease = ZoenModuleSessionLease()
  let gate = ZoenProcessSessionGate<Session> {
    try await probe.close($0.id)
    if $0 === first { await closed.signal() }
  }
  try await gate.open(session: first, lease: lease) { await probe.open(1) }
  let recovery = Task {
    try await gate.finishOwned(session: first, lease: lease) {
      await started.signal()
      await release.wait()
      // A pending erase may need its recovery Client after the old graceful
      // seal completed. It remains the only owner until this callback drains.
      await probe.open(1)
      try await probe.close(1)
      await effects.record()
    }
  }
  await started.wait()
  let replacing = Task {
    try await gate.open(session: second, lease: ZoenModuleSessionLease()) {
      check(await effects.count == 1, "New Client opened before accepted recovery drained")
      await probe.open(2)
    }
  }
  await closed.wait()
  await release.signal()
  try await recovery.value
  try await replacing.value
  check(!lease.isLive)
  try await gate.close(session: second) { try await probe.close(2) }
  let count = await probe.count(); check(count.0 == 3 && count.1 == 3)
}

private func latestClosedModuleKeepsItsRecoveryLease() async throws {
  let probe = Probe(), session = Session(1), lease = ZoenModuleSessionLease()
  let gate = ZoenProcessSessionGate<Session> { try await probe.close($0.id) }
  try await gate.open(session: session, lease: lease) { await probe.open(1) }
  try await gate.close(session: session) { try await probe.close(1) }
  try await gate.finishOwned(session: session, lease: lease) {
    await probe.open(1)
    try await probe.close(1)
  }
  check(lease.isLive)
  lease.retire()
  var rejected = false
  do { try await gate.finishOwned(session: session, lease: lease) { preconditionFailure("Retired module recovered") } }
  catch ZoenPushGateError.retired { rejected = true }
  check(rejected)
}

private func bridgeAndOSWakeShareOnlyTheCurrentModule() async throws {
  let probe = Probe(), entered = Entered(), release = Entered(), effects = Effects()
  let first = Session(1), lease = ZoenModuleSessionLease()
  let gate = ZoenProcessSessionGate<Session> { try await probe.close($0.id) }
  try await gate.open(session: first, lease: lease) { await probe.open(1) }
  let bridge = Task {
    await gate.runWake(session: first, lease: lease) { session, cold, work in
      check(session === first && !cold && work.isLive)
      await entered.signal()
      await release.wait()
      guard work.isLive else { throw CancellationError() }
      await effects.record()
      return .newData
    }
  }
  await entered.wait()
  let os = await gate.runWake(make: { preconditionFailure("OS duplicate opened another Client") }) { _, _, _ in preconditionFailure() }
  let duplicate = await gate.runWake(session: first, lease: lease) { _, _, _ in preconditionFailure("Bridge duplicate presented again") }
  check(os == .noData && duplicate == .noData)
  await release.signal()
  check(await bridge.value == .newData)
  check(await effects.count == 1)
  try await gate.close(session: first) { try await probe.close(1) }
  let closed = await gate.runWake(session: first, lease: lease) { _, _, _ in preconditionFailure("Bridge reopened a closed owner") }
  check(closed == .noData)
  let second = Session(2)
  try await gate.open(session: second, lease: ZoenModuleSessionLease()) { await probe.open(2) }
  let stale = await gate.runWake(session: first, lease: lease) { _, _, _ in preconditionFailure("Old callback reached a different module") }
  check(stale == .noData && !lease.isLive)
  try await gate.close(session: second) { try await probe.close(2) }
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
    try fetchedEvents(URL(fileURLWithPath: CommandLine.arguments[1]))
    try await ownership()
    try await staleCloseCannotReenterAnOlderSession()
    try await forceAndErasureCanEscalateAGracefulSeal()
    try await transferDrainsLatestModuleRecoveryAfterClose()
    try await latestClosedModuleKeepsItsRecoveryLease()
    try await bridgeAndOSWakeShareOnlyTheCurrentModule()
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
