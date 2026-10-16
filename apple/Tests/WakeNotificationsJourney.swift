import Foundation

// Adapter fixture matching the SpaceSummary fields consumed by this pure policy.
struct SpaceSummary {
    enum Kind { case direct, group }
    struct Author { let name: String }
    var id = "chat"
    var title = "Team"
    var kind = Kind.group
    var lastAuthor: Author? = Author(name: "Ada")
    var lastPreview = "Reply"
    var unread: UInt32 = 4
    var threadsUnread: UInt32 = 2
    var mentions: UInt32 = 0
}

@main
struct WakeNotificationsJourney {
    static func main() {
        let initial = SpaceSummary()
        let before = WakeNotifications.snapshot([initial])
        func notes(_ space: SpaceSummary, muted: Set<String> = []) -> [WakeNotifications.Note] {
            WakeNotifications.notes(before: before, after: [space], muted: muted)
        }
        precondition(notes(initial).isEmpty, "Unchanged unread must not notify")
        var reply = initial
        reply.threadsUnread += 1
        precondition(notes(reply).count == 1, "Thread-only reply must notify with unchanged chat unread")
        precondition(notes(reply).first?.body == "Ada: Reply", "Local preview must be preserved")
        reply.mentions += 1
        precondition(notes(reply).count == 1, "Mentioned followed reply must produce one notification")
        precondition(notes(reply, muted: [reply.id]).isEmpty, "Muted thread replies must stay silent")
        var mention = initial
        mention.mentions += 1
        precondition(notes(mention).count == 1, "New mention must notify even if unread totals are unchanged")
        var read = initial
        read.unread = 0
        read.threadsUnread = 0
        precondition(notes(read).isEmpty, "Read watermark changes must not notify")
        var chat = initial
        chat.unread += 1
        precondition(notes(chat).count == 1, "Chat unread growth must still notify")
        precondition(WakeNotifications.notes(before: WakeNotifications.snapshot([reply]), after: [reply], muted: []).isEmpty,
                     "Repeated wake must not duplicate a notification")
        precondition(WakeNotifications.notes(before: [:], after: [reply], muted: []).count == 1,
                     "New chat with unread followed replies must notify")
        print("PASS 10 wake notification regressions: thread-only replies, mentions, mute, reads and repeat wakes")
    }
}
