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
    var onActions: (() -> Void)? = nil
    var onIntent: (MessageSwipeIntent) -> Void

    /// Past this the release counts.
    static let threshold: CGFloat = 64
    /// The back swipe's zone (from the screen's leading edge).
    static let edgeZone: CGFloat = 24

    @State private var x: CGFloat = 0
    @State private var pull: CGFloat = 0    // the finger's signed pull (x stays 0 under Reduce Motion)
    @State private var locked: Bool?        // nil: undecided; true: horizontal; false: let go
    @State private var armed: MessageSwipeIntent?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityVoiceOverEnabled) private var voiceOver

    private var progress: CGFloat { min(1, abs(pull) / (Self.threshold * 0.9)) }

    func body(content: Content) -> some View {
        content
            .offset(x: reduceMotion ? 0 : x)
            .background(alignment: .trailing) {
                // Left pull: the reply arrow waits on the right.
                icon("arrowshape.turn.up.left.fill", active: armed == .reply)
                    .opacity(pull < 0 ? Double(progress) : 0)
                    .scaleEffect(reduceMotion ? 1 : (pull < 0 ? 0.4 + 0.6 * progress : 0.4))
                    .padding(.trailing, 6)
            }
            .background(alignment: .leading) {
                // Right pull: the thread glyph waits on the left.
                icon("bubble.left.and.text.bubble.right.fill", active: armed == .thread)
                    .opacity(pull > 0 ? Double(progress) : 0)
                    .scaleEffect(reduceMotion ? 1 : (pull > 0 ? 0.4 + 0.6 * progress : 0.4))
                    .padding(.leading, 6)
            }
            .simultaneousGesture(drag, isEnabled: enabled)
            .modifier(SwipeActionsForVoiceOver(on: voiceOver && enabled, onIntent: onIntent, onActions: onActions))
    }

    private func icon(_ name: String, active: Bool) -> some View {
        Image(systemName: name)
            .font(.system(size: 15, weight: .semibold))
            .foregroundStyle(active ? Color.white : Palette.textSecondary)
            .frame(width: 34, height: 34)
            .background(active ? Palette.action : Palette.textPrimary.opacity(0.08), in: .circle)
            // A small pop when the pull counts, so the eye gets the tick the finger feels.
            .scaleEffect(active && !reduceMotion ? 1.12 : 1)
            .animation(reduceMotion ? nil : .spring(response: 0.25, dampingFraction: 0.6), value: active)
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
                pull = raw < 0 ? -shown : shown
                // Reduce Motion: the bubble stays put; the icon alone shows the pull.
                x = reduceMotion ? 0 : pull
                let now: MessageSwipeIntent? = a >= Self.threshold ? (raw < 0 ? .reply : .thread) : nil
                if now != armed {
                    if now != nil { Haptics.selectionTick() }
                    armed = now
                }
            }
            .onEnded { v in
                var fire = locked == true ? armed : nil
                // A quick flick counts even short of the line (Telegram): most of the way
                // there, and heading well past it.
                if fire == nil, locked == true {
                    let raw = v.translation.width, ahead = v.predictedEndTranslation.width
                    if abs(raw) >= Self.threshold * 0.55, abs(ahead) >= Self.threshold * 1.8, (raw < 0) == (ahead < 0) {
                        fire = raw < 0 ? .reply : .thread
                    }
                }
                locked = nil
                armed = nil
                withAnimation(reduceMotion ? nil : .spring(response: 0.32, dampingFraction: 0.72)) { x = 0; pull = 0 }
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
    var onActions: (() -> Void)?
    func body(content: Content) -> some View {
        if on {
            content
                .accessibilityAction(named: Text("Reply")) { onIntent(.reply) }
                .accessibilityAction(named: Text("Abrir respostas")) { onIntent(.thread) }
                .accessibilityAction(named: Text("Message actions")) { onActions?() }
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
                    .frame(width: 44, height: 44)
                    .background(Palette.textPrimary.opacity(0.08), in: .circle)
            }
            .buttonStyle(.plain)
            .keyboardShortcut(.cancelAction)
            .accessibilityLabel(Text("Cancel reply"))
            .accessibilityIdentifier("reply-cancel")
        }
        .padding(.horizontal, 14).padding(.vertical, 8)
        .glassEffect(.regular, in: .rect(cornerRadius: 18, style: .continuous))
        .padding(.horizontal, 12)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("reply-bar")
        .accessibilityAction(.escape, onClose)
    }
}

/// Under a thread's root (Slack/Ando): faces of who replied, "N replies", when the last one
/// came, and an unread dot when there's something I haven't seen in a thread I follow.
struct ThreadRepliesChip: View {
    let count: UInt32
    var faces: [Persona] = []
    var lastReplyMs: Int64 = 0
    var unread: UInt32 = 0
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
                Text(count == 1 ? String(localized: "1 resposta") : String(localized: "\(Int(count)) respostas"))
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(Palette.action)
                if lastReplyMs > 0 {
                    Text(RodaTime.short(lastReplyMs))
                        .font(.caption2)
                        .foregroundStyle(Palette.textTertiary)
                }
                if unread > 0 {
                    Circle().fill(Palette.action).frame(width: 7, height: 7)
                        .transition(.scale.combined(with: .opacity))
                        .accessibilityLabel("\(Int(unread)) não lidas")
                }
                Image(systemName: "chevron.right").font(.system(size: 9, weight: .bold)).foregroundStyle(Palette.action.opacity(0.7))
            }
            .padding(.horizontal, 8).padding(.vertical, 4)
            .background(Palette.action.opacity(0.1), in: .capsule)
            .animation(Motion.wabi, value: unread)
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
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.scenePhase) private var scenePhase
    let spaceId: String
    let rootId: String
    var focusedEntryId: String? = nil
    @State private var entries: [TimelineEntry] = []
    @State private var draft = ThreadReplyDraft()
    @FocusState private var focused: Bool

    private var following: Bool { entries.first?.threadFollowing ?? false }

    var body: some View {
        NavigationStack {
            ScrollViewReader { proxy in
                ScrollView {
                    VStack(alignment: .leading, spacing: 4) {
                        if let root = entries.first {
                            row(root, previous: nil)
                                .padding(.bottom, 6)
                                .id(root.id)
                            HStack(spacing: 8) {
                                Text(root.threadReplies == 0 ? String(localized: "Nenhuma resposta ainda") :
                                        root.threadReplies == 1 ? String(localized: "1 resposta") : String(localized: "\(Int(root.threadReplies)) respostas"))
                                    .font(.caption.weight(.semibold))
                                    .foregroundStyle(Palette.textSecondary)
                                Rectangle().fill(Palette.textPrimary.opacity(0.1)).frame(height: 1)
                            }
                            .padding(.vertical, 6)
                            .accessibilityIdentifier("thread-count")
                            ForEach(Array(entries.dropFirst().enumerated()), id: \.element.id) { i, e in
                                row(e, previous: i == 0 ? nil : entries[i])
                                    .id(e.id)
                                    .transition(reduceMotion ? .opacity : .scale(scale: 0.92, anchor: .bottom).combined(with: .opacity))
                            }
                        }
                        Color.clear.frame(height: 6).id("thread-bottom")
                    }
                    .padding(.horizontal, 14)
                    .padding(.top, 8)
                }
                .defaultScrollAnchor(.bottom)
                .onChange(of: entries.count) { _, _ in withAnimation(reduceMotion ? nil : .snappy) { proxy.scrollTo("thread-bottom", anchor: .bottom) } }
                .task(id: ChatTimeline.location(of: focusedEntryId ?? "", in: entries)) {
                    guard let id = focusedEntryId, entries.contains(where: { $0.id == id }) else { return }
                    do { try await Task.sleep(for: .milliseconds(80)) } catch { return }
                    proxy.scrollTo(id, anchor: .center)
                }
            }
            .background(Palette.background.ignoresSafeArea())
            .safeAreaBar(edge: .bottom, spacing: 0) {
                VStack(alignment: .leading, spacing: 0) {
                    Toggle(isOn: $draft.alsoInChat) {
                        Text("Também enviar na conversa").font(.footnote)
                    }
                    .toggleStyle(.checkmark)
                    .padding(.horizontal, 18)
                    .padding(.bottom, 4)
                    .accessibilityIdentifier("thread-also-in-chat")
                    Composer(text: $draft.text, placeholder: String(localized: "Escreva uma resposta"), focused: $focused, busy: false) {
                        guard draft.submit({ text, broadcast in
                            model.perform({ try model.core.sendThreadReply(spaceId: spaceId, text: text, rootId: rootId, alsoInChat: broadcast) }) != nil
                        }) else { return }
                        Haptics.send()
                        reload()
                    }
                }
            }
            .navigationTitle(Text("Respostas"))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button { dismiss() } label: { Text("Fechar") }
                        .accessibilityIdentifier("thread-close")
                }
                ToolbarItem(placement: .primaryAction) {
                    Button {
                        Haptics.selectionTick()
                        model.perform { try model.core.setThreadFollow(spaceId: spaceId, rootId: rootId, follow: !following) }
                        reload()
                    } label: {
                        Label(following ? "Acompanhando" : "Acompanhar respostas", systemImage: following ? "bell.fill" : "bell")
                            .contentTransition(.symbolEffect(.replace))
                    }
                    .accessibilityIdentifier("thread-follow")
                }
            }
        }
        .accessibilityIdentifier("thread-sheet")
        .task(id: model.revision) { reload() }
        .onChange(of: scenePhase) { _, phase in if phase == .active { reload() } }
        .task {
            try? await Task.sleep(for: .milliseconds(450))
            focused = true
        }
    }

    private func row(_ e: TimelineEntry, previous: TimelineEntry?) -> some View {
        let grouped = previous?.author.id == e.author.id && e.atMs - (previous?.atMs ?? 0) < 5 * 60_000
        return Group {
            if case .message(let text, let card) = e.kind {
                MessageRow(author: e.author, text: text, card: card, voice: e.voice.map(VoiceNoteRef.init), atMs: e.atMs, delivery: e.delivery, grouped: grouped,
                           isDirect: false, onOpenItem: { model.openApp($0) })
                    // Thread replies live only here, so long-press must still offer Copy.
                    .contextMenu {
                        if !text.isEmpty {
                            Button { copyToPasteboard(text) } label: { Label("Copy", systemImage: "doc.on.doc") }
                        }
                    }
                    .padding(.top, grouped ? 0 : 8)
            }
        }
    }

    private func reload() {
        var loaded = (try? model.core.thread(spaceId: spaceId, rootId: rootId)) ?? []
        if let observed = ChatTimeline.threadReadThrough(in: loaded, active: scenePhase == .active) {
            do {
                try model.core.markThreadReadUpTo(spaceId: spaceId, rootId: rootId, seq: observed)
                model.refresh()
                loaded = (try? model.core.thread(spaceId: spaceId, rootId: rootId)) ?? loaded
            } catch let error as CoreError {
                // perform() refreshes even on failure, restarting this revision task
                // with the same unread root forever. Leave revision unchanged here.
                model.show(.init(kind: .error, text: error.message))
            } catch {
                model.show(.init(kind: .error, text: error.localizedDescription))
            }
        }
        withAnimation(reduceMotion ? nil : Motion.wabi) {
            entries = loaded
        }
    }
}

struct ThreadRef: Identifiable, Equatable {
    let id: String
    var entryId: String? = nil
}

/// Checkbox-style toggle (Slack's "Also send to channel").
struct CheckmarkToggleStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Button { configuration.isOn.toggle() } label: {
            HStack(spacing: 8) {
                Image(systemName: configuration.isOn ? "checkmark.square.fill" : "square")
                    .foregroundStyle(configuration.isOn ? Palette.action : Palette.textTertiary)
                    .contentTransition(.symbolEffect(.replace))
                configuration.label.foregroundStyle(Palette.textSecondary)
            }
        }
        .buttonStyle(.plain)
        .animation(Motion.wabi, value: configuration.isOn)
    }
}

extension ToggleStyle where Self == CheckmarkToggleStyle {
    static var checkmark: CheckmarkToggleStyle { .init() }
}

/// Threads I follow across all my chats, newest reply first. Unread and mentions are
/// distinct (Ando): a dot for unread, "@N" for mentions.
struct ThreadsInboxView: View {
    @Environment(AppModel.self) private var model
    @State private var threads: [ThreadSummary] = []
    @State private var open: ThreadSummary?

    var body: some View {
        List(threads) { t in
            Button { open = t } label: {
                HStack(alignment: .top, spacing: 12) {
                    Avatar(persona: t.root.author, size: 36)
                    VStack(alignment: .leading, spacing: 3) {
                        HStack {
                            Text(t.spaceTitle).font(.subheadline.weight(t.root.threadUnread > 0 ? .bold : .semibold))
                            Spacer()
                            Text(RodaTime.short(t.root.threadLastReplyMs)).font(.caption2).foregroundStyle(Palette.textTertiary)
                        }
                        Text(MessageReplies.snippet(t.root)).font(.subheadline).foregroundStyle(Palette.textPrimary).lineLimit(1)
                        if let last = t.lastReply {
                            Text("\(last.author.isMe ? String(localized: "You") : last.author.name): \(MessageReplies.snippet(last))")
                                .font(.footnote).foregroundStyle(Palette.textSecondary).lineLimit(1)
                        }
                    }
                    if t.mentions > 0 {
                        Text("@\(Int(t.mentions))").font(.caption.weight(.bold)).foregroundStyle(.white)
                            .padding(.horizontal, 6).frame(height: 20).background(Palette.action, in: Capsule())
                    } else if t.root.threadUnread > 0 {
                        Circle().fill(Palette.action).frame(width: 9, height: 9).padding(.top, 6)
                    }
                }
            }
            .buttonStyle(.plain)
        }
        .listStyle(.plain)
        .overlay { if threads.isEmpty { InkEmptyState(pose: .map, title: String(localized: "Nenhuma resposta acompanhada ainda")) } }
        .navigationTitle("Respostas")
        .task(id: model.revision) { threads = model.core.threads() }
        .threadPanel(item: $open) { t in ThreadSheet(spaceId: t.spaceId, rootId: t.root.id) }
    }
}

extension View {
    /// A thread opens as a side panel where there's room (Mac, iPad, landscape) and as a
    /// sheet on iPhone (Ando).
    func threadPanel<Item: Identifiable, Panel: View>(item: Binding<Item?>, @ViewBuilder panel: @escaping (Item) -> Panel) -> some View {
        modifier(ThreadPanel(item: item, panel: panel))
    }
}

private struct ThreadPanel<Item: Identifiable, Panel: View>: ViewModifier {
    @Binding var item: Item?
    let panel: (Item) -> Panel
    @Environment(\.horizontalSizeClass) private var size

    private var usesInspector: Bool {
        #if os(macOS)
        true
        #else
        size == .regular
        #endif
    }

    func body(content: Content) -> some View {
        if usesInspector {
            content.inspector(isPresented: Binding(get: { item != nil }, set: { if !$0 { item = nil } })) {
                if let item { panel(item).id(item.id).inspectorColumnWidth(min: 320, ideal: 380, max: 480) }
            }
        } else {
            content.sheet(item: $item) { panel($0).presentationDetents([.large]) }
        }
    }
}

@MainActor
private func copyToPasteboard(_ text: String) {
    #if os(iOS)
    UIPasteboard.general.string = text
    #elseif os(macOS)
    NSPasteboard.general.clearContents()
    NSPasteboard.general.setString(text, forType: .string)
    #endif
}
