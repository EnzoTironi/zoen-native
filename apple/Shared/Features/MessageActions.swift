import SwiftUI
import RodaCore

/// Long-press a message: the chat dims, the bubble's actions rise from where it sits, with
/// the reaction bar on top (WhatsApp order: react, reply, forward, copy, edit, delete).
/// Everything here calls generic core verbs; nothing knows what the chat is for.
struct MessageActionsOverlay: View {
    let spaceId: String
    let entry: TimelineEntry
    var onReply: () -> Void
    var onForward: () -> Void
    var onEdit: () -> Void
    var onDelete: () -> Void
    var onClose: () -> Void
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shown = false

    static let quick = ["👍", "❤️", "😂", "😮", "😢", "🙏"]

    private var text: String {
        if case .message(let text, _) = entry.kind { return text }
        return ""
    }
    private var mine: Bool { entry.author.isMe }
    private var myReaction: String? { entry.reactions.first(where: \.mine)?.emoji }

    var body: some View {
        ZStack(alignment: mine ? .bottomTrailing : .bottomLeading) {
            Color.black.opacity(shown ? 0.28 : 0)
                .ignoresSafeArea()
                .onTapGesture(perform: close)
                .accessibilityLabel("Close")
                .accessibilityAddTraits(.isButton)
            VStack(alignment: mine ? .trailing : .leading, spacing: 12) {
                reactionBar
                    .scaleEffect(shown ? 1 : 0.6, anchor: mine ? .bottomTrailing : .bottomLeading)
                    .opacity(shown ? 1 : 0)
                actions
                    .offset(y: shown ? 0 : 16)
                    .opacity(shown ? 1 : 0)
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 96)
        }
        .onAppear {
            Haptics.menuOpen()
            withAnimation(reduceMotion ? .linear(duration: 0.12) : Motion.wabi) { shown = true }
        }
    }

    private var reactionBar: some View {
        HStack(spacing: 4) {
            ForEach(Self.quick, id: \.self) { emoji in
                Button {
                    Haptics.selectionTick()
                    let next = emoji == myReaction ? nil : emoji
                    model.perform { try model.core.react(spaceId: spaceId, messageId: entry.id, emoji: next) }
                    close()
                } label: {
                    Text(emoji)
                        .font(.system(size: 28))
                        .frame(width: 44, height: 44)
                        .background { if emoji == myReaction { Circle().fill(Palette.action.opacity(0.18)) } }
                }
                .buttonStyle(PressScale())
                .accessibilityLabel(emoji == myReaction ? "Remove \(emoji)" : "React \(emoji)")
            }
        }
        .padding(6)
        .background(.regularMaterial, in: Capsule())
        .shadow(color: .black.opacity(0.12), radius: 16, y: 6)
    }

    private var actions: some View {
        VStack(spacing: 0) {
            row("Reply", "arrowshape.turn.up.left") { onReply(); close() }
            if !entry.deleted {
                row("Forward", "arrowshape.turn.up.right") { onForward(); close() }
                if !text.isEmpty {
                    row("Copy", "doc.on.doc") {
                        #if os(iOS)
                        UIPasteboard.general.string = text
                        #else
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(text, forType: .string)
                        #endif
                        close()
                    }
                }
                if mine && !text.isEmpty && VoiceNoteRef.parse(text) == nil {
                    row("Edit", "pencil") { onEdit(); close() }
                }
                if mine {
                    row("Delete for everyone", "trash", role: .destructive) { onDelete(); close() }
                }
            }
        }
        .frame(width: 248)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 24, style: .continuous))
        .shadow(color: .black.opacity(0.12), radius: 16, y: 6)
    }

    private func row(_ title: LocalizedStringKey, _ symbol: String, role: ButtonRole? = nil, action: @escaping () -> Void) -> some View {
        Button(role: role, action: action) {
            HStack {
                Text(title)
                Spacer()
                Image(systemName: symbol)
            }
            .font(.body)
            .foregroundStyle(role == .destructive ? Palette.danger : Palette.textPrimary)
            .padding(.horizontal, 16)
            .frame(height: 46)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func close() {
        withAnimation(reduceMotion ? .linear(duration: 0.1) : Motion.wabi) { shown = false }
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(180))
            onClose()
        }
    }
}

/// Reactions under a bubble: one chip per emoji with its count; mine is tinted, and tapping
/// it takes mine back.
struct ReactionChips: View {
    let spaceId: String
    let entry: TimelineEntry
    @Environment(AppModel.self) private var model

    var body: some View {
        HStack(spacing: 4) {
            ForEach(entry.reactions, id: \.emoji) { r in
                Button {
                    Haptics.selectionTick()
                    model.perform { try model.core.react(spaceId: spaceId, messageId: entry.id, emoji: r.mine ? nil : r.emoji) }
                } label: {
                    HStack(spacing: 3) {
                        Text(r.emoji).font(.system(size: 14))
                        if r.people.count > 1 {
                            Text("\(r.people.count)").font(.caption2.weight(.semibold).monospacedDigit())
                        }
                    }
                    .padding(.horizontal, 7)
                    .frame(height: 24)
                    .background(r.mine ? Palette.action.opacity(0.16) : Palette.otherBubble, in: Capsule())
                    .overlay(Capsule().strokeBorder(Palette.hairline, lineWidth: 0.5))
                }
                .buttonStyle(PressScale())
                .accessibilityLabel("\(r.emoji), \(r.people.map(\.name).joined(separator: ", "))")
                .transition(.scale(scale: 0.4).combined(with: .opacity))
            }
        }
        .animation(Motion.wabi, value: entry.reactions)
    }
}

/// Pick a chat to forward a message to.
struct ForwardSheet: View {
    let fromSpaceId: String
    let messageId: String
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List(model.spaces.filter { $0.id != fromSpaceId }, id: \.id) { space in
                Button {
                    Haptics.send()
                    model.perform { try model.core.forwardMessage(fromSpaceId: fromSpaceId, messageId: messageId, toSpaceId: space.id) }
                    dismiss()
                } label: {
                    HStack(spacing: 12) {
                        if let c = space.counterpart { Avatar(persona: c, size: 36) } else { GroupAvatar(space: space, size: 36) }
                        Text(space.title).foregroundStyle(Palette.textPrimary)
                    }
                }
            }
            .navigationTitle("Forward to…")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
        }
    }
}

/// A message deleted for everyone keeps its place, quietly.
struct DeletedBubble: View {
    let mine: Bool

    var body: some View {
        Label("This message was deleted", systemImage: "nosign")
            .font(.subheadline.italic())
            .foregroundStyle(Palette.textTertiary)
            .padding(.horizontal, 14)
            .frame(minHeight: 38)
            .overlay(RoundedRectangle(cornerRadius: 24, style: .continuous).strokeBorder(Palette.hairline, lineWidth: 0.8))
            .padding(.leading, mine ? 0 : 44)
    }
}
