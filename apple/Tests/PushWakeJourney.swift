import Foundation

private struct PushJourneyFailure: Error, CustomStringConvertible {
    let description: String
}

@MainActor
private final class NotificationFixture: PushNotificationSystem {
    var current = PushPermission.authorized
    var registrations = 0
    var unregistrations = 0
    var holdPermission = false
    var waiting: CheckedContinuation<PushPermission, Never>?
    var onRegister: (() -> Void)?
    func permission() async throws -> PushPermission {
        if holdPermission { return await withCheckedContinuation { waiting = $0 } }
        return current
    }
    func requestPermission() async throws -> PushPermission { try await permission() }
    func registerForRemoteNotifications() { registrations += 1; onRegister?() }
    func unregisterForRemoteNotifications() { unregistrations += 1 }
}

/// These fixtures exercise the adapter boundary; they are not a relay or APNs proof.
@MainActor
private final class RegistrationFixture: PushRegistrationStore {
    var deviceSessionId: String? = "device-A"
    var current = PushRegistrationSnapshot(supported: false, state: .unregistered,
        environment: .sandbox, topic: "xyz.tironi.zoen", deliveryAvailable: false)
    var reply = PushRegistrationSnapshot(supported: true, state: .queued,
        environment: .sandbox, topic: "xyz.tironi.zoen", deliveryAvailable: false)
    var writes: [PushRegistrationRequest] = []
    var rejectWrites = false
    var holdSync = false
    var waiting: CheckedContinuation<Bool, Never>?
    var synchronized: [PushWake] = []
    func snapshot() throws -> PushRegistrationSnapshot { current }
    func setRegistration(_ request: PushRegistrationRequest) throws -> PushRegistrationSnapshot {
        if rejectWrites { throw PushJourneyFailure(description: "Fixture storage unavailable") }
        writes.append(request)
        current = reply
        return current
    }
    func synchronize(_ wake: PushWake) async throws -> Bool {
        guard wake.spaceId == "sp_allowed" else { return false }
        synchronized.append(wake)
        if holdSync { return await withCheckedContinuation { waiting = $0 } }
        return true
    }
}

@main
struct PushWakeJourney {
    static func check(_ condition: Bool, _ message: String) throws {
        if !condition { throw PushJourneyFailure(description: message) }
    }
    static func payload(cursor: String = "1", space: String = "sp_allowed") -> [AnyHashable: Any] {
        ["aps": ["content-available": 1], "zoen": ["v": 1, "space_id": space, "event_seq": cursor]]
    }
    @MainActor
    static func until(_ ready: () -> Bool) async throws {
        for _ in 0..<200 {
            if ready() { return }
            try await Task.sleep(for: .milliseconds(5))
        }
        throw PushJourneyFailure(description: "Fixture callback did not complete")
    }
    static func note(_ message: String) { FileHandle.standardOutput.write(Data((message + "\n").utf8)) }

    @MainActor
    static func main() async throws {
        let example = Data(#"{"aps":{"content-available":1},"zoen":{"v":1,"space_id":"sp_01jqtest","event_seq":"18446744073709551615"}}"#.utf8)
        try check(PushWake.decode(example)?.eventSequence == UInt64.max, "Encoder example lost UInt64 precision")
        try check(PushWake.decode(payload(cursor: "9007199254740993"))?.eventSequence == 9_007_199_254_740_993,
                  "Cursor above 2^53 was rounded")
        try check(PushWake.decode(payload(space: String(repeating: "A", count: 128))) != nil, "Maximum valid Space was rejected")
        for value in ["", "0", "01", "-1", "+1", " 1", "1 ", "1.0", "1e3", "18446744073709551616"] {
            try check(PushWake.decode(payload(cursor: value)) == nil, "Invalid decimal cursor was accepted")
        }
        for value in ["", "../other", "space id", "space\0id", "ação", String(repeating: "a", count: 129)] {
            try check(PushWake.decode(payload(space: value)) == nil, "Invalid Space was accepted")
        }
        for value in [true, 1.0, "1", 0, 2, NSNull()] as [Any] {
            var malformed = payload()
            var body = malformed["zoen"] as! [String: Any]
            body["v"] = value
            malformed["zoen"] = body
            try check(PushWake.decode(malformed) == nil, "Version coerced a boolean, floating value or wrong type")
            malformed = payload()
            malformed["aps"] = ["content-available": value]
            try check(PushWake.decode(malformed) == nil, "Content-available coerced a wrong type")
        }
        for field in ["v", "space_id", "event_seq"] {
            var malformed = payload()
            var body = malformed["zoen"] as! [String: Any]
            body.removeValue(forKey: field)
            malformed["zoen"] = body
            try check(PushWake.decode(malformed) == nil, "Missing wake field was accepted")
        }
        var numeric = payload()
        numeric["zoen"] = ["v": 1, "space_id": "sp_allowed", "event_seq": UInt64.max]
        try check(PushWake.decode(numeric) == nil, "Numeric cursor bypassed exact-string contract")
        for wire in [#"{"aps":{"content-available":1},"zoen":{"v":true,"space_id":"sp_allowed","event_seq":"1"}}"#,
                     #"{"aps":{"content-available":1},"zoen":{"v":1.0,"space_id":"sp_allowed","event_seq":"1"}}"#,
                     #"{"aps":{"content-available":1.0},"zoen":{"v":1,"space_id":"sp_allowed","event_seq":"1"}}"#] {
            try check(PushWake.decode(Data(wire.utf8)) == nil, "JSON boolean/floating version was coerced")
        }
        var action = payload()
        action["approval"] = "anything"
        try check(PushWake.decode(action) == nil, "Unexpected action-bearing payload was accepted")
        try check(PushWake.decode(Data(repeating: 32, count: 4097)) == nil, "Oversized wake was accepted")
        note("PASS exact versioned wake fields and UInt64 cursors without coercion")

        let config = PushConfiguration(apsEntitlement: "development", topic: "xyz.tironi.zoen")!
        try check(config.environment == .sandbox, "Signed development entitlement did not map to sandbox")
        try check(PushConfiguration(apsEntitlement: "production", topic: "xyz.tironi.zoen")?.environment == .production,
                  "Production entitlement was changed")
        for entitlement in [nil, "sandbox", "Production", "", "production\n"] {
            try check(PushConfiguration(apsEntitlement: entitlement, topic: "xyz.tironi.zoen") == nil, "Unknown signing environment was accepted")
        }
        for topic in [nil, "", "xyz", "xyz.*", "xyz..zoen", ".xyz", "xyz/zoen", "xyz.zoen\n", String(repeating: "a", count: 256)] {
            try check(PushConfiguration(apsEntitlement: "production", topic: topic) == nil, "Invalid topic was accepted")
        }
        for count in [1, 16, 32, 64, 256] {
            try check(PushConfiguration.tokenHex(Data(repeating: 0xab, count: count)) == String(repeating: "ab", count: count),
                      "APNs variable-length bytes were truncated or changed")
        }
        try check(PushConfiguration.tokenHex(Data()) == nil && PushConfiguration.tokenHex(Data(count: 257)) == nil,
                  "Invalid token length was accepted")
        note("PASS signed environment mapping, bounded topic and variable-length APNs bytes")

        let system = NotificationFixture()
        let store = RegistrationFixture()
        let coordinator = PushCoordinator(system: system, configuration: config, store: store)
        await coordinator.foreground()
        try check(coordinator.registration?.supported == false && system.registrations == 0 && store.writes.isEmpty,
                  "Unsupported Core fabricated registration or requested APNs")
        store.current = store.reply
        await coordinator.foreground()
        try check(system.registrations == 1 && coordinator.registration?.state == .queued,
                  "APNs request changed the real Core snapshot")
        coordinator.receivedToken(Data([0xab, 0xcd]))
        try check(store.writes.last?.token == "abcd" && coordinator.registration?.state == .queued,
                  "Token acceptance fabricated registered state")
        coordinator.registrationChanged()
        try check(store.writes.count == 1, "An unchanged callback wrote a duplicate registration")
        store.current = PushRegistrationSnapshot(supported: true, state: .registered,
            environment: .sandbox, topic: config.topic, deliveryAvailable: false)
        coordinator.registrationChanged()
        try check(coordinator.registration == store.current && coordinator.registration?.deliveryAvailable == false,
                  "Coordinator invented provider readiness")
        system.current = .denied
        await coordinator.foreground()
        try check(store.writes.last?.token == nil && system.unregistrations > 0, "Denied permission did not persist removal")
        note("PASS truthful Core snapshots, idempotent forwarding and denied-permission removal")

        let staleSystem = NotificationFixture()
        staleSystem.holdPermission = true
        let staleStore = RegistrationFixture()
        let stale = PushCoordinator(system: staleSystem, configuration: config, store: staleStore)
        let permissionTask = Task { await stale.foreground() }
        try await until { staleSystem.waiting != nil }
        staleStore.deviceSessionId = "device-B"
        staleSystem.waiting?.resume(returning: .authorized)
        await permissionTask.value
        try check(stale.permission == .unknown && stale.registration == nil && staleSystem.registrations == 0,
                  "A previous device's authorization changed the replacement session")
        let wake = PushWake.decode(payload())!
        staleStore.holdSync = true
        let syncTask = Task { await stale.receiveWake(wake) }
        try await until { staleStore.waiting != nil }
        let detached = stale.detach()
        staleStore.waiting?.resume(returning: true)
        let staleResult = await syncTask.value
        try check(detached && !staleResult && stale.registration == nil, "A sync callback survived logout")
        try check(staleStore.writes.last?.token == nil, "Logout omitted its durable removal request")
        let foreign = PushWake.decode(payload(space: "sp_foreign"))!
        try check(await coordinator.receiveWake(foreign) == false, "Membership-denied wake entered sync")
        note("PASS device/logout epochs and membership-denied wake handling")

        let retrySystem = NotificationFixture()
        let retryStore = RegistrationFixture()
        retryStore.current = retryStore.reply
        let retried = PushCoordinator(system: retrySystem, configuration: config, store: retryStore, retryDelays: [.zero, .zero])
        retrySystem.onRegister = { [weak retried] in retried?.registrationFailed() }
        await retried.foreground()
        try await until { retrySystem.registrations == 3 }
        try await Task.sleep(for: .milliseconds(20))
        try check(retrySystem.registrations == 3 && retried.failure == .token && retryStore.writes.isEmpty,
                  "APNs failure retries were unbounded or fabricated storage")
        retryStore.rejectWrites = true
        try check(!retried.detach() && retried.registration == nil, "Failed unregister was reported as persisted")
        note("PASS bounded in-memory retries and truthful unregister failure")
        note("RESULT 5 push boundary journeys passed; 0 failed")
    }
}
