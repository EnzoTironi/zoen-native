import Foundation

// Adapter fixtures for the core DTO fields consumed by the presentation policy.
struct TimelineEntry: Equatable {
    struct Author: Equatable { var isMe = false }
    enum Kind: Equatable { case message, system }
    var id: String
    var seq: UInt64 = 0
    var author = Author()
    var kind = Kind.message
    var inThread: String?
    var alsoInChat = false
    var threadUnread: UInt32 = 0
}

struct ThreadSummary {
    var spaceId: String
    var root: TimelineEntry
}

@main
struct ChatTimelineJourney {
    static func main() {
        var failures = [String]()
        var checks = 0
        func check(_ condition: Bool, _ name: String) {
            checks += 1
            if !condition { failures.append(name); print("FAIL: \(name)") }
        }
        let old = TimelineEntry(id: "old")
        let first = TimelineEntry(id: "first-unread")
        let second = TimelineEntry(id: "second-unread")
        let hidden = TimelineEntry(id: "hidden", inThread: "root")
        let otherHidden = TimelineEntry(id: "other-hidden", inThread: "root")
        let broadcast = TimelineEntry(id: "broadcast", inThread: "root", alsoInChat: true)
        let mine = TimelineEntry(id: "mine", author: .init(isMe: true))
        let myBroadcast = TimelineEntry(id: "my-broadcast", author: .init(isMe: true), inThread: "root", alsoInChat: true)
        let system = TimelineEntry(id: "system", kind: .system)

        check(ChatTimeline.firstUnread(in: [old, first, hidden], unread: 1) == first.id,
              "Hidden tail must not consume the chat unread suffix")
        check(ChatTimeline.firstUnread(in: [old, first, hidden, second, otherHidden], unread: 2) == first.id,
              "Interleaved thread-only replies must not move the opening divider")
        check(ChatTimeline.firstUnread(in: [old, broadcast], unread: 1) == broadcast.id,
              "Broadcast reply must be an unread chat boundary")
        check(ChatTimeline.firstUnread(in: [old, broadcast, hidden], unread: 1) == broadcast.id,
              "Hidden reply after a broadcast must not hide its unread boundary")
        check(ChatTimeline.firstUnread(in: [old, first, mine, myBroadcast, system, hidden], unread: 1) == first.id,
              "Only incoming chat-visible messages consume unread")
        check(ChatTimeline.firstUnread(in: [first, hidden], unread: 0) == nil,
              "Zero unread must not create a divider")
        check(ChatTimeline.firstUnread(in: [mine, myBroadcast, system, hidden], unread: 2) == nil,
              "No incoming chat messages must leave the divider absent")
        check(ChatTimeline.firstUnread(in: [first, broadcast, hidden], unread: 99) == first.id,
              "An oversized unread count must clamp to incoming chat history")
        check(ChatTimeline.visible([old, hidden, broadcast, myBroadcast, system]).map(\.id)
                == [old.id, broadcast.id, myBroadcast.id, system.id],
              "Chat visibility must keep broadcasts and system rows but exclude thread-only replies")
        check(ChatTimeline.added(in: [old, broadcast], after: [old]).map(\.id) == [broadcast.id],
              "Incoming broadcast must trigger new-message tracking")
        check(ChatTimeline.added(in: [old, myBroadcast], after: [old]).first?.author.isMe == true,
              "My broadcast must trigger sent-message following")
        check(ChatTimeline.added(in: [old, hidden], after: [old]).isEmpty,
              "Hidden replies must not move or increment the chat")
        check(ChatTimeline.added(in: [old, first, hidden, broadcast, system], after: [old]).map(\.id)
                == [first.id, broadcast.id, system.id],
              "New-message tracking must match the rendered chat rows")
        var edited = broadcast
        edited.author.isMe = true
        check(ChatTimeline.added(in: [old, edited], after: [old, broadcast]).isEmpty,
              "Existing IDs must not be treated as appended messages")

        let firstThread = ThreadSummary(spaceId: "chat-one", root: old)
        let secondThread = ThreadSummary(spaceId: "chat-two", root: old)
        check(firstThread.id != secondThread.id,
              "Same root ID in different chats must have different inbox identities")
        check(firstThread.id == ThreadSummary(spaceId: "chat-one", root: old).id,
              "Refreshing a thread must preserve its inbox identity")
        check(ThreadSummary(spaceId: "a|b", root: .init(id: "c")).id
                != ThreadSummary(spaceId: "a", root: .init(id: "b|c")).id,
              "Composite thread identities must not collide at a delimiter")

        check(ChatTimeline.location(of: hidden.id, in: [old, hidden, broadcast])
                == .thread(root: "root", entry: hidden.id),
              "Search and quote jumps to hidden replies must open their thread anchor")
        check(ChatTimeline.location(of: broadcast.id, in: [old, hidden, broadcast]) == .chat(broadcast.id),
              "Broadcast search and quote jumps must keep their rendered chat anchor")
        check(ChatTimeline.location(of: old.id, in: [old, hidden]) == .chat(old.id),
              "Root search jumps must stay in the chat")
        check(ChatTimeline.location(of: "not-loaded-yet", in: [old]) == nil,
              "An unavailable search target must wait for its actual history entry")

        var root = old
        root.seq = 4
        root.threadUnread = 1
        var observed = hidden
        observed.seq = 7
        check(ChatTimeline.threadReadThrough(in: [root, observed], active: true) == 7,
              "Read watermark must stop at the observed reply snapshot, not a later arrival")
        check(ChatTimeline.threadReadThrough(in: [root, observed], active: false) == nil,
              "An inactive thread view must never mark replies read")
        check(ChatTimeline.threadReadThrough(in: [root], active: true) == nil,
              "A missing reply snapshot must not write a watermark")
        root.threadUnread = 0
        check(ChatTimeline.threadReadThrough(in: [root, observed], active: true) == nil,
              "A quiet thread must not repeatedly refresh its read watermark")

        if !failures.isEmpty {
            print("FAIL: \(failures.count) of \(checks) chat/thread presentation regressions")
            exit(1)
        }
        print("PASS: \(checks) chat/thread presentation regressions")
    }
}
