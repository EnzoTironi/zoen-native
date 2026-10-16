import Foundation
#if canImport(RodaCore)
import RodaCore
#endif

/// What a wake turns into once this device has synced and decrypted: one local
/// notification per unmuted chat that gained unread chat messages or followed replies. Pure, so it is unit-tested.
enum WakeNotifications {
    struct Note: Equatable { let spaceId: String; let title: String; let body: String }

    struct Unread: Equatable {
        let chat: UInt32
        let threads: UInt32
        let mentions: UInt32

        init(_ space: SpaceSummary) {
            chat = space.unread
            threads = space.threadsUnread
            mentions = space.mentions
        }
    }

    static func snapshot(_ spaces: [SpaceSummary]) -> [String: Unread] {
        Dictionary(spaces.map { ($0.id, Unread($0)) }, uniquingKeysWith: { first, _ in first })
    }

    static func notes(before: [String: Unread], after: [SpaceSummary], muted: Set<String>) -> [Note] {
        after.compactMap { s in
            let previous = before[s.id]
            guard !muted.contains(s.id),
                  s.unread > (previous?.chat ?? 0)
                    || s.threadsUnread > (previous?.threads ?? 0)
                    || s.mentions > (previous?.mentions ?? 0) else { return nil }
            let who = s.lastAuthor?.name ?? s.title
            let title = s.kind == .direct ? who : s.title
            let body = s.kind == .direct || s.lastAuthor == nil ? s.lastPreview : "\(who): \(s.lastPreview)"
            return Note(spaceId: s.id, title: title, body: body)
        }
    }
}

