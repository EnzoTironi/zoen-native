#if canImport(RodaCore)
import RodaCore
#endif

/// Keep rendering, unread boundaries and arrival tracking on the same chat projection.
enum ChatTimeline {
    enum Location: Equatable {
        case chat(String)
        case thread(root: String, entry: String)
    }

    static func isVisible(_ entry: TimelineEntry) -> Bool {
        entry.inThread == nil || entry.alsoInChat
    }

    static func visible(_ entries: [TimelineEntry]) -> [TimelineEntry] {
        entries.filter(isVisible)
    }

    static func firstUnread(in entries: [TimelineEntry], unread: UInt32) -> String? {
        let incoming = entries.filter {
            guard !$0.author.isMe, isVisible($0) else { return false }
            if case .message = $0.kind { return true }
            return false
        }
        return incoming.suffix(Int(unread)).first?.id
    }

    static func added(in entries: [TimelineEntry], after previous: [TimelineEntry]) -> [TimelineEntry] {
        let known = Set(previous.map(\.id))
        return entries.filter { !known.contains($0.id) && isVisible($0) }
    }

    /// Read only the reply snapshot this active view actually loaded. The core clamps
    /// this bound to a reply, so a later arrival cannot be consumed by the read call.
    static func threadReadThrough(in entries: [TimelineEntry], active: Bool) -> UInt64? {
        guard active, let root = entries.first, root.threadUnread > 0 else { return nil }
        return entries.dropFirst().map(\.seq).max()
    }

    /// Hidden replies have no scroll anchor in the chat; open their thread instead.
    static func location(of id: String, in entries: [TimelineEntry]) -> Location? {
        guard let entry = entries.first(where: { $0.id == id }) else { return nil }
        if let root = entry.inThread, !isVisible(entry) { return .thread(root: root, entry: entry.id) }
        return .chat(entry.id)
    }
}

// Message IDs are scoped to a chat. An array keeps the pair unambiguous even if
// either component contains a separator used by a string-concatenated key.
extension ThreadSummary: Identifiable {
    public var id: [String] { [spaceId, root.id] }
}
