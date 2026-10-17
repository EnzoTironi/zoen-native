import Foundation
#if canImport(RodaCore)
import RodaCore
#endif

/// A content-free wake becomes one local notification per unmuted chat with a
/// newly received, still-unread chat message or followed-thread reply.
enum WakeNotifications {
    struct Note: Equatable { let spaceId: String; let title: String; let body: String }

    /// Confirmed log sequences are zero-based and contiguous within each chat:
    /// eventCount is the next sequence, not an unread count or a timestamp.
    static func snapshot(_ spaces: [SpaceSummary]) -> [String: UInt64] {
        Dictionary(spaces.map { ($0.id, $0.eventCount) }, uniquingKeysWith: { first, _ in first })
    }

    static func notes(before: [String: UInt64], after: [SpaceSummary], muted: Set<String>,
                      timeline: (String) -> [TimelineEntry]) -> [Note] {
        after.compactMap { space in
            let boundary = before[space.id] ?? 0
            guard !muted.contains(space.id), space.eventCount > boundary,
                  space.unread > 0 || space.threadsUnread > 0 else { return nil }
            let entries = timeline(space.id)
            let unread = unreadIds(in: entries, chatUnread: space.unread)
            guard let entry = entries.last(where: { unread.contains($0.id) && $0.seq >= boundary && !$0.deleted }) else { return nil }
            let who = entry.author.name
            let text = preview(entry)
            return Note(spaceId: space.id, title: space.kind == .direct ? who : space.title,
                        body: space.kind == .direct ? text : "\(who): \(text)")
        }
    }

    private static func unreadIds(in entries: [TimelineEntry], chatUnread: UInt32) -> Set<String> {
        var chat = [TimelineEntry]()
        var replies = [String: [TimelineEntry]]()
        for entry in entries {
            guard !entry.author.isMe, case .message = entry.kind else { continue }
            if entry.inThread == nil || entry.alsoInChat { chat.append(entry) }
            if let root = entry.inThread { replies[root, default: []].append(entry) }
        }
        // Match native unread accounting before excluding deleted candidates. Dropping
        // deleted entries first would pull an older, already-read message into a suffix.
        var unread = Set(chat.suffix(Int(chatUnread)).map(\.id))
        for root in entries where root.inThread == nil && root.threadFollowing && root.threadUnread > 0 {
            unread.formUnion((replies[root.id] ?? []).suffix(Int(root.threadUnread)).map(\.id))
        }
        return unread
    }

    private static func preview(_ entry: TimelineEntry) -> String {
        if let voice = entry.voice {
            return voice.transcript.isEmpty ? String(localized: "Voice message") : voice.transcript
        }
        if case .message(let text, let card) = entry.kind {
            if !text.isEmpty { return text }
            if let card { return "\(card.kindLabel) · \(card.title)" }
        }
        return String(localized: "New message")
    }
}
