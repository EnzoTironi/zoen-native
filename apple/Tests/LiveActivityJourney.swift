import Foundation

private struct JourneyFailure: Error, CustomStringConvertible {
    let description: String
}

@main
struct LiveActivityJourney {
    static func main() throws {
        var passed = 0
        func expect(_ condition: Bool, _ name: String) throws {
            guard condition else { throw JourneyFailure(description: name) }
            passed += 1
            print("PASS \(name)")
        }
        let now = Date(timeIntervalSince1970: 1_000)
        var state = LiveActivityState<String>()
        state.set(space: "chat", identity: "ana", kind: "typing", until: now.addingTimeInterval(6))
        state.set(space: "chat", identity: "bia", kind: "typing", until: now.addingTimeInterval(8))
        state.set(space: "other", identity: "ana", kind: "typing", until: now.addingTimeInterval(10))
        try expect(state.spaces["chat"]?.count == 2, "concurrent typists coexist")
        state.remove(space: "chat", identity: "ana")
        try expect(state.spaces["chat"]?["bia"] != nil, "stop removes only its sender")
        try expect(state.spaces["other"]?["ana"] != nil, "stop is scoped to its space")
        state.set(space: "chat", identity: "bia", kind: "typing", until: now.addingTimeInterval(12))
        state.expire(now: now.addingTimeInterval(8))
        try expect(state.spaces["chat"]?["bia"] != nil, "old deadline cannot expire refreshed typing")
        try expect(state.nextExpiry == now.addingTimeInterval(10), "next expiry follows earliest active sender")
        state.expire(now: now.addingTimeInterval(10))
        try expect(state.spaces["other"] == nil, "expiry removes empty spaces")
        try expect(state.spaces["chat"]?["bia"] != nil, "independent later deadline survives")
        state.remove(identity: "bia")
        try expect(state.spaces.isEmpty && state.nextExpiry == nil, "offline sender removed across spaces")
        state.set(space: "chat", identity: "ana", kind: "processing", until: now.addingTimeInterval(60))
        state.expire(now: now.addingTimeInterval(60))
        try expect(state.spaces.isEmpty, "non-typing activity also expires")
        state.set(space: "chat", identity: "ana", kind: "typing", until: now.addingTimeInterval(6))
        state.reset()
        try expect(state.spaces.isEmpty && state.nextExpiry == nil, "reconnect reset clears every transient sender")
        print("RESULT \(passed) passed; 0 failed")
    }
}
