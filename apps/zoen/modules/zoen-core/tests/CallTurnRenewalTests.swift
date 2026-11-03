import Foundation

@MainActor
private final class RenewalFixture {
  var generation: UInt8 = 1
  var live = true
  var invalidated = false
  var replacement = false
  var confirmed = true
  var held: CheckedContinuation<Void, Never>?
  var heldRead: CheckedContinuation<Void, Never>?
  var holdFinalRead = false
  var issuerFails = false
  var requests = 0
  var reads = 0
  var credentials = ZoenCallTurn(urls: ["turns:relay.invalid:5349?transport=tcp"], username: "synthetic", credential: "synthetic", expiresAtMs: Int64.max)

  func record() -> ZoenCallRecord {
    ZoenCallRecord(callId: replacement ? "replacement" : "call", invitation: "signed", caller: "alice", callerDevice: "a1", callee: "bob", media: "audio", invitedAtMs: 0, expiresAtMs: 1, invitationConfirmed: true, acceptedDevice: "b1", acceptedAtMs: 1, acceptanceConfirmed: confirmed, endedAtMs: invalidated ? 1 : nil, endReason: invalidated ? "hang_up" : nil, invalidated: invalidated)
  }
  func port() -> ZoenCallPort {
    ZoenCallPort(scope: .init(identity: "bob", device: "b1", space: "dm", call: "call", invitation: "signed"), locale: "en",
      relayReady: { true }, current: { await self.read() }, transport: { [] }, send: { _, _ in }, prepareAudio: {}, releaseAudio: {},
      turn: { try await self.issue() }, action: { _ in })
  }
  func read() async -> ZoenCallRecord {
    reads += 1
    let snapshot = record()
    if holdFinalRead && reads == 2 { await withCheckedContinuation { heldRead = $0 } }
    return snapshot
  }
  func issue() async throws -> ZoenCallTurn {
    requests += 1
    await withCheckedContinuation { held = $0 }
    if issuerFails { throw ZoenCallFailure.unavailable }
    return credentials
  }
  func renew() async throws -> ZoenCallTurn {
    let expected = generation
    return try await port().renewedTurn { self.live && self.generation == expected }
  }
}

@main
struct CallTurnRenewalTests {
  @MainActor
  static func until(_ predicate: () -> Bool) async {
    for _ in 0..<10_000 { if predicate() { return }; await Task.yield() }
    preconditionFailure("Renewal failed to reach its held issuer")
  }
  @MainActor
  static func main() async throws {
    let callee = RenewalFixture()
    let success = Task { try await callee.renew() }
    await until { callee.held != nil }
    callee.held?.resume(); callee.held = nil
    let fresh = try await success.value
    precondition(fresh.username == "synthetic" && callee.requests == 1 && callee.reads == 2)

    for edge in ["generation", "termination", "owner", "stop", "unconfirmed"] {
      let fixture = RenewalFixture()
      let renewal = Task { try await fixture.renew() }
      await until { fixture.held != nil }
      switch edge {
      case "generation": fixture.generation += 1
      case "termination": fixture.invalidated = true
      case "owner": fixture.replacement = true
      case "stop": fixture.live = false
      default: fixture.confirmed = false
      }
      fixture.held?.resume(); fixture.held = nil
      do { _ = try await renewal.value; preconditionFailure("Stale renewal admitted: \(edge)") }
      catch ZoenCallFailure.retired {}
    }

    let finalRead = RenewalFixture(); finalRead.holdFinalRead = true
    let delayed = Task { try await finalRead.renew() }
    await until { finalRead.held != nil }
    finalRead.held?.resume(); finalRead.held = nil
    await until { finalRead.heldRead != nil }
    finalRead.generation += 1
    finalRead.heldRead?.resume(); finalRead.heldRead = nil
    do { _ = try await delayed.value; preconditionFailure("Final canonical await lost generation fence") }
    catch ZoenCallFailure.retired {}

    for credentials in [
      ZoenCallTurn(urls: [], username: "u", credential: "c", expiresAtMs: Int64.max),
      ZoenCallTurn(urls: ["stun:relay.invalid"], username: "u", credential: "c", expiresAtMs: Int64.max),
      ZoenCallTurn(urls: ["turns:relay.invalid"], username: "", credential: "c", expiresAtMs: Int64.max),
      ZoenCallTurn(urls: ["turns:relay.invalid"], username: "u", credential: "", expiresAtMs: Int64.max),
      ZoenCallTurn(urls: ["turns:relay.invalid"], username: "u", credential: "c", expiresAtMs: 0),
    ] {
      let fixture = RenewalFixture(); fixture.credentials = credentials
      let renewal = Task { try await fixture.renew() }
      await until { fixture.held != nil }
      fixture.held?.resume(); fixture.held = nil
      do { _ = try await renewal.value; preconditionFailure("Invalid TURN config admitted") }
      catch ZoenCallFailure.unavailable {}
    }
    let unavailable = RenewalFixture(); unavailable.issuerFails = true
    let failure = Task { try await unavailable.renew() }
    await until { unavailable.held != nil }
    unavailable.held?.resume(); unavailable.held = nil
    do { _ = try await failure.value; preconditionFailure("Issuer failure returned fallback credentials") }
    catch ZoenCallFailure.unavailable {}
    precondition(unavailable.requests == 1)

    let stopped = RenewalFixture(); stopped.live = false
    do { _ = try await stopped.renew(); preconditionFailure("Stopped owner reached issuer") }
    catch ZoenCallFailure.retired {}
    precondition(stopped.requests == 0)
    print("PASS scoped native TURN renewal: callee, generation, termination, owner, consent and TLS-only failure")
  }
}
