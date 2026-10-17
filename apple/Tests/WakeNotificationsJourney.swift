import Foundation

// Adapter fixtures for the core DTO fields consumed by this pure policy.
struct SpaceSummary {
    enum Kind { case direct, group }
    struct Author { let name: String }
    var id = "chat"
    var title = "Team"
    var kind = Kind.group
    var lastAuthor: Author? = Author(name: "Ada")
    var lastPreview = "Old preview"
    var unread: UInt32 = 0
    var threadsUnread: UInt32 = 1
    var mentions: UInt32 = 0
    var eventCount: UInt64 = 10
}

struct TimelineEntry {
    struct Author { var isMe = false; var name = "Ada" }
    struct Card { let kindLabel: String; let title: String }
    struct Voice { let transcript: String }
    enum Kind { case message(String, Card?), system }
    var id: String
    var seq: UInt64
    var author = Author()
    var kind = Kind.message("Reply", nil)
    var inThread: String?
    var alsoInChat = false
    var threadFollowing = false
    var threadUnread: UInt32 = 0
    var deleted = false
    var voice: Voice?
}

@main
struct WakeNotificationsJourney {
    static func main() {
        var failures = [String]()
        var checks = 0
        func check(_ condition: Bool, _ name: String) {
            checks += 1
            if !condition { failures.append(name); print("FAIL: \(name)") }
        }
        let initial = SpaceSummary()
        let before = WakeNotifications.snapshot([initial])
        let root = TimelineEntry(id: "root", seq: 1, author: .init(isMe: true, name: "You"),
                                 threadFollowing: true, threadUnread: 1)
        let oldReply = TimelineEntry(id: "old", seq: 9, kind: .message("Old reply", nil), inThread: root.id)
        let newReply = TimelineEntry(id: "new", seq: 10, kind: .message("Followed reply", nil), inThread: root.id)
        var grownRoot = root
        grownRoot.threadUnread = 2
        var grown = initial
        grown.eventCount = 11
        grown.threadsUnread = 2
        var timelineReads = 0
        func notes(_ space: SpaceSummary, _ entries: [TimelineEntry], muted: Set<String> = []) -> [WakeNotifications.Note] {
            WakeNotifications.notes(before: before, after: [space], muted: muted) { _ in
                timelineReads += 1
                return entries
            }
        }
        check(notes(initial, [root, oldReply]).isEmpty, "Unchanged unread and log boundary must not notify")
        check(timelineReads == 0, "An unchanged chat must not load its timeline")
        check(notes(grown, [grownRoot, oldReply, newReply]).count == 1,
              "Thread-only reply must notify with unchanged chat unread")
        check(notes(grown, [grownRoot, oldReply, newReply]).first?.body == "Ada: Followed reply",
              "Notification preview must come from the actual eligible reply")
        var mentioned = grown
        mentioned.mentions = 1
        check(notes(mentioned, [grownRoot, oldReply, newReply]).count == 1,
              "Mentioned followed reply must produce one notification")
        let readsBeforeMute = timelineReads
        check(notes(grown, [grownRoot, oldReply, newReply], muted: [grown.id]).isEmpty,
              "Muted thread replies must stay silent")
        check(timelineReads == readsBeforeMute, "Muted chats must not load notification history")
        var read = grown
        read.threadsUnread = 0
        check(notes(read, [grownRoot, oldReply, newReply]).isEmpty, "Fully read chats must not notify")

        let newChatMessage = TimelineEntry(id: "chat-new", seq: 10, kind: .message("New message", nil))
        var chat = initial
        chat.eventCount = 11
        chat.unread = 1
        chat.threadsUnread = 0
        check(notes(chat, [newChatMessage]).count == 1, "Chat unread growth must still notify")
        var chatMention = chat
        chatMention.mentions = 1
        check(notes(chatMention, [newChatMessage]).count == 1, "A new chat mention must notify once")
        check(WakeNotifications.notes(before: WakeNotifications.snapshot([grown]), after: [grown], muted: [],
                                      timeline: { _ in [grownRoot, oldReply, newReply] }).isEmpty,
              "Repeated wake must not duplicate a notification")
        var newSpace = chat
        newSpace.eventCount = 1
        let firstMessage = TimelineEntry(id: "first", seq: 0)
        check(WakeNotifications.notes(before: [:], after: [newSpace], muted: [], timeline: { _ in [firstMessage] }).count == 1,
              "Sequence zero in a newly received chat must notify")

        var replacedRead = initial
        replacedRead.eventCount = 11
        check(notes(replacedRead, [root, oldReply, newReply]).count == 1,
              "Remote read plus newer reply must notify even when unread totals are unchanged")
        let oldChatMessage = TimelineEntry(id: "chat-old", seq: 9)
        check(notes(chat, [oldChatMessage, newChatMessage]).count == 1,
              "Chat watermark movement must not hide a new unread chat message")

        var unfollowed = initial
        unfollowed.unread = 1
        unfollowed.threadsUnread = 0
        var followed = unfollowed
        followed.threadsUnread = 1
        let oldBroadcast = TimelineEntry(id: "old-broadcast", seq: 9, inThread: root.id, alsoInChat: true)
        check(WakeNotifications.notes(before: WakeNotifications.snapshot([unfollowed]), after: [followed], muted: [],
                                      timeline: { _ in [root, oldBroadcast] }).isEmpty,
              "Following an already-notified broadcast must not notify again")
        followed.eventCount = 11
        let system = TimelineEntry(id: "system", seq: 10, kind: .system)
        check(WakeNotifications.notes(before: WakeNotifications.snapshot([unfollowed]), after: [followed], muted: [],
                                      timeline: { _ in [root, oldBroadcast, system] }).isEmpty,
              "An unrelated event plus follow-state change must not renotify old replies")

        let unfollowedRoot = TimelineEntry(id: "unfollowed-root", seq: 2, threadUnread: 1)
        let unrelated = TimelineEntry(id: "unrelated", seq: 11, author: .init(name: "Bob"),
                                      kind: .message("Unfollowed thread", nil), inThread: unfollowedRoot.id)
        var wrongPreview = grown
        wrongPreview.eventCount = 12
        wrongPreview.lastAuthor = .init(name: "Bob")
        wrongPreview.lastPreview = "Unfollowed thread"
        check(notes(wrongPreview, [grownRoot, unfollowedRoot, oldReply, newReply, unrelated]).first?.body == "Ada: Followed reply",
              "A newer unfollowed reply must never supply a followed-thread notification body")
        var unchangedUnread = initial
        unchangedUnread.eventCount = 12
        check(notes(unchangedUnread, [root, unfollowedRoot, oldReply, unrelated]).isEmpty,
              "Only old followed unread plus a new unfollowed reply must remain silent")

        let broadcast = TimelineEntry(id: "broadcast", seq: 10, inThread: root.id, alsoInChat: true)
        var both = grown
        both.unread = 1
        check(notes(both, [grownRoot, oldReply, broadcast]).count == 1,
              "A broadcast counted in both unread domains must notify only once")
        let mine = TimelineEntry(id: "mine", seq: 11, author: .init(isMe: true, name: "You"))
        var afterMine = chat
        afterMine.eventCount = 12
        check(notes(afterMine, [newChatMessage, mine]).first?.body == "Ada: New message",
              "My later message must not mask the new incoming unread candidate")
        check(notes(unchangedUnread, [root, oldReply, mine]).isEmpty,
              "My message must not renotify older unread replies")
        var deleted = newChatMessage
        deleted.deleted = true
        check(notes(chat, [oldChatMessage, deleted]).isEmpty,
              "Deleted incoming entries consume the unread suffix but must not notify")
        check(notes(chat, [oldChatMessage, system]).isEmpty,
              "A new non-message event must not renotify an older unread message")

        var direct = chat
        direct.kind = .direct
        check(notes(direct, [newChatMessage]).first == .init(spaceId: direct.id, title: "Ada", body: "New message"),
              "Direct notifications use the eligible author and omit the group prefix")
        let voice = TimelineEntry(id: "voice", seq: 10, voice: .init(transcript: "Voice transcript"))
        check(notes(chat, [voice]).first?.body == "Ada: Voice transcript", "Voice notifications use the local transcript")
        let silentVoice = TimelineEntry(id: "voice-empty", seq: 10, voice: .init(transcript: ""))
        check(notes(chat, [silentVoice]).first?.body == "Ada: Voice message", "Voice without a transcript uses a readable label")
        let card = TimelineEntry(id: "card", seq: 10, kind: .message("", .init(kindLabel: "Page", title: "Trip")))
        check(notes(chat, [card]).first?.body == "Ada: Page · Trip", "Attached-card notifications retain a readable preview")
        var otherSpace = chat
        otherSpace.id = "other-chat"
        let perSpace = WakeNotifications.notes(before: [initial.id: 11, otherSpace.id: 10], after: [chat, otherSpace], muted: [],
                                              timeline: { _ in [newChatMessage] })
        check(perSpace.map(\.spaceId) == [otherSpace.id], "Sequence boundaries must stay scoped to their chat")

        if !failures.isEmpty { print("FAIL: \(failures.count) of \(checks) wake notification regressions"); exit(1) }
        print("PASS: \(checks) wake notification regressions")
    }
}
