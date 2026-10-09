import SwiftUI
import RodaCore

// MARK: - Drag a message to reply (Telegram) or to reply in a thread (Slack)

/// Which way a message was pulled.
enum MessageSwipeIntent: Equatable { case reply, thread }

/// Drag a message LEFT → inline reply: the bubble follows the finger with resistance, a reply
/// arrow fades and grows in on the right, a tick at the threshold, and on release the
/// composer shows the quote. Drag RIGHT → reply in its thread: a thread glyph on the left,
/// release opens the thread.
///
/// Never fights the system: drags that start in the left edge zone (the back swipe) are
/// ignored, and only a mostly-horizontal motion locks in, so vertical scrolling stays free.
struct MessageSwipe: ViewModifier {
    var enabled: Bool
    var onIntent: (MessageSwipeIntent) -> Void

    /// Past this the release counts.
    static let threshold: CGFloat = 64
    /// The back swipe's zone (from the screen's leading edge).
    static let edgeZone: CGFloat = 24

    @State private var x: CGFloat = 0
    @State private var locked: Bool?        // nil: undecided; true: horizontal; false: let go
    @State private var armed: MessageSwipeIntent?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityVoiceOverEnabled) private var voiceOver

    private var progress: CGFloat { min(1, abs(x) / Self.threshold) }

    func body(content: Content) -> some View {
        content
            .offset(x: x)
            .background(alignment: .trailing) {
                // Left pull: the reply arrow waits on the right.
                icon("arrowshape.turn.up.left.fill", active: armed == .reply)
                    .opacity(x < 0 ? Double(progress) : 0)
                    .scaleEffect(x < 0 ? 0.4 + 0.6 * progress : 0.4)
                    .padding(.trailing, 6)
            }
            .background(alignment: .leading) {
                // Right pull: the thread glyph waits on the left.
                icon("bubble.left.and.text.bubble.right.fill", active: armed == .thread)
                    .opacity(x > 0 ? Double(progress) : 0)
                    .scaleEffect(x > 0 ? 0.4 + 0.6 * progress : 0.4)
                    .padding(.leading, 6)
            }
            .simultaneousGesture(drag, isEnabled: enabled)
            .modifier(SwipeActionsForVoiceOver(on: voiceOver && enabled, onIntent: onIntent))
    }

    private func icon(_ name: String, active: Bool) -> some View {
        Image(systemName: name)
            .font(.system(size: 15, weight: .semibold))
            .foregroundStyle(active ? Color.white : Palette.textSecondary)
            .frame(width: 34, height: 34)
            .background(active ? Palette.action : Palette.textPrimary.opacity(0.08), in: .circle)
            .animation(.spring(response: 0.25, dampingFraction: 0.6), value: active)
            .accessibilityHidden(true)
    }

    private var drag: some Gesture {
        DragGesture(minimumDistance: 12, coordinateSpace: .global)
            .onChanged { v in
                if locked == nil {
                    // The back swipe owns the edge; and only a clearly sideways pull is ours.
                    if v.startLocation.x < Self.edgeZone { locked = false; return }
                    let dx = abs(v.translation.width), dy = abs(v.translation.height)
                    locked = dx > dy * 1.8 && dx > 10
                }
                guard locked == true else { return }
                let raw = v.translation.width
                // Free up to the threshold, then a rubber band.
                let a = abs(raw)
                let shown = a <= Self.threshold ? a * 0.9 : Self.threshold * 0.9 + (a - Self.threshold) * 0.22
                x = reduceMotion ? 0 : (raw < 0 ? -shown : shown)
                let now: MessageSwipeIntent? = a >= Self.threshold ? (raw < 0 ? .reply : .thread) : nil
                if now != armed {
                    if now != nil { Haptics.selectionTick() }
                    armed = now
                }
            }
            .onEnded { _ in
                let fire = locked == true ? armed : nil
                locked = nil
                armed = nil
                withAnimation(.spring(response: 0.32, dampingFraction: 0.72)) { x = 0 }
                if let fire {
                    Haptics.open()
                    onIntent(fire)
                }
            }
    }
}

/// Reply / Reply in thread as VoiceOver actions on the message. Only while VoiceOver runs:
/// actions on the row turn it into one element, which would swallow the sender's name and
/// the cards' buttons for everyone else (taps on the name stopped opening the profile).
private struct SwipeActionsForVoiceOver: ViewModifier {
    var on: Bool
    var onIntent: (MessageSwipeIntent) -> Void
    func body(content: Content) -> some View {
        if on {
            content
                .accessibilityAction(named: Text("Reply")) { onIntent(.reply) }
                .accessibilityAction(named: Text("Reply in thread")) { onIntent(.thread) }
        } else {
            content
        }
    }
}

// MARK: - The quote inside a reply bubble, and the bar above the composer

struct ReplyQuoteView: View {
    let quote: ReplyQuote
    var mine: Bool

    var body: some View {
        HStack(spacing: 8) {
            RoundedRectangle(cornerRadius: 1.5).fill(mine ? Palette.myBubbleText.opacity(0.8) : Color(hex: quote.author.tintHex))
                .frame(width: 3)
            VStack(alignment: .leading, spacing: 1) {
                Text(quote.author.isMe ? String(localized: "You") : quote.author.name)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(mine ? Palette.myBubbleText : Color(hex: quote.author.tintHex))
                Text(quote.text)
                    .font(.caption)
                    .lineLimit(2)
                    .foregroundStyle(mine ? Palette.myBubbleText.opacity(0.8) : Palette.textSecondary)
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 6).padding(.horizontal, 8)
        .background((mine ? Palette.myBubbleText : Palette.textPrimary).opacity(0.08), in: .rect(cornerRadius: 10, style: .continuous))
        .fixedSize(horizontal: false, vertical: true)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("reply-quote")
    }
}

/// "Respondendo a Marina" above the composer, with the quoted line and ×.
struct ReplyComposerBar: View {
    let entry: TimelineEntry
    var onClose: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "arrowshape.turn.up.left.fill")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(Palette.action)
            RoundedRectangle(cornerRadius: 1.5).fill(Palette.action).frame(width: 3, height: 32)
            VStack(alignment: .leading, spacing: 1) {
                Text(entry.author.isMe ? String(localized: "Replying to yourself") : String(localized: "Replying to \(entry.author.name)"))
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(Palette.action)
                Text(MessageReplies.snippet(entry))
                    .font(.caption)
                    .lineLimit(1)
                    .foregroundStyle(Palette.textSecondary)
            }
            Spacer(minLength: 0)
            Button(action: onClose) {
                Image(systemName: "xmark")
                    .font(.system(size: 12, weight: .bold))
                    .foregroundStyle(Palette.textSecondary)
                    .frame(width: 28, height: 28)
                    .background(Palette.textPrimary.opacity(0.08), in: .circle)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text("Cancel reply"))
            .accessibilityIdentifier("reply-cancel")
        }
        .padding(.horizontal, 14).padding(.vertical, 8)
        .glassEffect(.regular, in: .rect(cornerRadius: 18, style: .continuous))
        .padding(.horizontal, 12)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("reply-bar")
    }
}

/// "3 respostas" under a thread's root: tiny faces of who replied, then the count.
struct ThreadRepliesChip: View {
    let count: UInt32
    var faces: [Persona] = []
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                HStack(spacing: -6) {
                    ForEach(Array(faces.prefix(3).enumerated()), id: \.offset) { _, p in
                        Avatar(persona: p, size: 18)
                            .overlay(Circle().strokeBorder(Palette.background, lineWidth: 1.5))
                    }
                }
                Text(count == 1 ? String(localized: "1 reply") : String(localized: "\(Int(count)) replies"))
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(Palette.action)
                Image(systemName: "chevron.right").font(.system(size: 9, weight: .bold)).foregroundStyle(Palette.action.opacity(0.7))
            }
            .padding(.horizontal, 8).padding(.vertical, 4)
            .background(Palette.action.opacity(0.1), in: .capsule)
        }
        .buttonStyle(PressScaleStyle())
        .accessibilityIdentifier("thread-chip")
    }
}

enum MessageReplies {
    static func snippet(_ e: TimelineEntry) -> String {
        guard case .message(let text, let card) = e.kind else { return "" }
        if let v = VoiceNoteRef.parse(text) { return v.transcript.isEmpty ? String(localized: "Voice message") : v.transcript }
        if text.isEmpty, let card { return "\(card.kindLabel) · \(card.title)" }
        return text
    }
}

/// A message's thread (Slack-style): the root on top, its replies under it, and a composer
/// that answers in the thread.
struct ThreadSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let spaceId: String
    let rootId: String
    @State private var entries: [TimelineEntry] = []
    @State private var draft = ""
    @FocusState private var focused: Bool

    var body: some View {
        NavigationStack {
            ScrollViewReader { proxy in
                ScrollView {
                    VStack(alignment: .leading, spacing: 4) {
                        if let root = entries.first {
                            row(root, previous: nil)
                                .padding(.bottom, 6)
                            HStack(spacing: 8) {
                                Text(root.threadReplies == 0 ? String(localized: "No replies yet") :
                                        root.threadReplies == 1 ? String(localized: "1 reply") : String(localized: "\(Int(root.threadReplies)) replies"))
                                    .font(.caption.weight(.semibold))
                                    .foregroundStyle(Palette.textSecondary)
                                Rectangle().fill(Palette.textPrimary.opacity(0.1)).frame(height: 1)
                            }
                            .padding(.vertical, 6)
                            .accessibilityIdentifier("thread-count")
                            ForEach(Array(entries.dropFirst().enumerated()), id: \.element.id) { i, e in
                                row(e, previous: i == 0 ? nil : entries[i])
                                    .id(e.id)
                                    .transition(.scale(scale: 0.92, anchor: .bottom).combined(with: .opacity))
                            }
                        }
                        Color.clear.frame(height: 6).id("thread-bottom")
                    }
                    .padding(.horizontal, 14)
                    .padding(.top, 8)
                }
                .defaultScrollAnchor(.bottom)
                .onChange(of: entries.count) { _, _ in withAnimation(.snappy) { proxy.scrollTo("thread-bottom", anchor: .bottom) } }
            }
            .background(Palette.background.ignoresSafeArea())
            .safeAreaBar(edge: .bottom, spacing: 0) {
                Composer(text: $draft, placeholder: String(localized: "Reply in thread"), focused: $focused, busy: false) {
                    let text = draft
                    draft = ""
                    if model.sendReply(text, to: rootId, thread: true, in: spaceId) { reload() }
                }
            }
            .navigationTitle(Text("Thread"))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button { dismiss() } label: { Text("Close") }
                        .accessibilityIdentifier("thread-close")
                }
            }
        }
        .accessibilityIdentifier("thread-sheet")
        .task(id: model.revision) { reload() }
        .task {
            try? await Task.sleep(for: .milliseconds(450))
            focused = true
        }
    }

    private func row(_ e: TimelineEntry, previous: TimelineEntry?) -> some View {
        let grouped = previous?.author.id == e.author.id && e.atMs - (previous?.atMs ?? 0) < 5 * 60_000
        return Group {
            if case .message(let text, let card) = e.kind {
                MessageRow(author: e.author, text: text, card: card, atMs: e.atMs, grouped: grouped,
                           isDirect: false, onOpenItem: { model.openApp($0) })
                    .padding(.top, grouped ? 0 : 8)
            }
        }
    }

    private func reload() {
        withAnimation(.spring(duration: 0.4, bounce: 0.2)) {
            entries = (try? model.core.thread(spaceId: spaceId, rootId: rootId)) ?? []
        }
    }
}

struct ThreadRef: Identifiable, Equatable { let id: String }
