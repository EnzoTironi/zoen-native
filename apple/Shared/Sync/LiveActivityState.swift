import Foundation

/// Ephemeral activity is independent for every (space, identity) pair.
/// The caller schedules one wake-up at `nextExpiry`, rather than one task per keystroke.
struct LiveActivityState<Kind> {
    struct Entry {
        let kind: Kind
        let until: Date
    }

    private(set) var spaces: [String: [String: Entry]] = [:]

    var nextExpiry: Date? {
        spaces.values.flatMap { $0.values.map(\.until) }.min()
    }

    mutating func set(space: String, identity: String, kind: Kind, until: Date) {
        spaces[space, default: [:]][identity] = Entry(kind: kind, until: until)
    }

    mutating func remove(space: String, identity: String) {
        spaces[space]?[identity] = nil
        if spaces[space]?.isEmpty == true { spaces[space] = nil }
    }

    mutating func remove(identity: String) {
        for space in Array(spaces.keys) { remove(space: space, identity: identity) }
    }

    mutating func expire(now: Date) {
        for space in Array(spaces.keys) {
            spaces[space] = spaces[space]?.filter { $0.value.until > now }
            if spaces[space]?.isEmpty == true { spaces[space] = nil }
        }
    }

    mutating func reset() { spaces.removeAll() }
}
