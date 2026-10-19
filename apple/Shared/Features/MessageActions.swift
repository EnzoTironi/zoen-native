import SwiftUI
import ClientCore

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
    @State private var choosingEmoji = false

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
        .sheet(isPresented: $choosingEmoji) {
            EmojiReactionPicker(currentEmoji: myReaction) { emoji in
                react(emoji)
            }
        }
        .onAppear {
            Haptics.menuOpen()
            withAnimation(reduceMotion ? .linear(duration: 0.12) : Motion.wabi) { shown = true }
        }
    }

    private var reactionBar: some View {
        HStack(spacing: 2) {
            ForEach(Self.quick, id: \.self) { emoji in
                Button {
                    react(emoji)
                } label: {
                    Text(emoji)
                        .font(.system(size: 28))
                        .frame(width: 38, height: 44)
                        .background { if emoji == myReaction { Circle().fill(Palette.action.opacity(0.18)) } }
                }
                .buttonStyle(PressScale())
                .accessibilityLabel(emoji == myReaction ? String(localized: "Remove \(emoji)") : String(localized: "React \(emoji)"))
            }
            Button { choosingEmoji = true } label: {
                Image(systemName: "plus")
                    .font(.title3.weight(.semibold))
                    .frame(width: 38, height: 44)
            }
            .buttonStyle(PressScale())
            .accessibilityLabel("Mais emojis")
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

    private func react(_ emoji: String) {
        Haptics.selectionTick()
        let next = emoji == myReaction ? nil : emoji
        model.perform { try model.core.react(spaceId: spaceId, messageId: entry.id, emoji: next) }
        choosingEmoji = false
        close()
    }

    private func close() {
        withAnimation(reduceMotion ? .linear(duration: 0.1) : Motion.wabi) { shown = false }
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(180))
            onClose()
        }
    }
}

/// Reaction chips open participant details, with explicit replace/remove actions.
struct ReactionChips: View {
    let spaceId: String
    let entry: TimelineEntry
    @State private var selectedReaction: String?

    var body: some View {
        HStack(spacing: 4) {
            ForEach(entry.reactions, id: \.emoji) { r in
                Button {
                    Haptics.selectionTick()
                    selectedReaction = r.emoji
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
                .accessibilityHint("Ver participantes e opções da reação")
                .transition(.scale(scale: 0.4).combined(with: .opacity))
            }
        }
        .animation(Motion.wabi, value: entry.reactions)
        .sheet(isPresented: Binding(
            get: { selectedReaction != nil },
            set: { if !$0 { selectedReaction = nil } }
        )) {
            ReactionDetailsSheet(spaceId: spaceId, entry: entry, initialEmoji: selectedReaction ?? "")
        }
    }
}

/// Full emoji entry uses the system keyboard, so skin tones, flags and joined emoji
/// are not limited to the curated palette.
private struct EmojiReactionPicker: View {
    let currentEmoji: String?
    var onSelect: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var typedEmoji = ""

    private static let groups: [(title: String, emojis: [String])] = [
        ("Rostos", ["😀", "😃", "😁", "🥹", "😊", "😍", "🥰", "😘", "😎", "🤔", "🤨", "😴", "😭", "😡", "🤯", "🥳", "🫠", "🫡", "🤗", "🤩", "😅"]),
        ("Gestos e pessoas", ["👍", "👎", "👏", "🙌", "🙏", "🤝", "💪", "👋", "✌️", "🤞", "🫶", "👌", "🤷", "🤦", "💃", "🕺", "👀", "🧠", "👑", "🫂", "💅"]),
        ("Corações e símbolos", ["❤️", "🧡", "💛", "💚", "💙", "💜", "🖤", "🤍", "🤎", "🩷", "🩵", "🩶", "💔", "💕", "💯", "✅", "❌", "❓", "❗️", "✨", "🔥"]),
        ("Animais e natureza", ["🐶", "🐱", "🐼", "🦊", "🦋", "🐝", "🌻", "🌹", "🌱", "🌈", "☀️", "🌙", "⭐️", "🌊"]),
        ("Comida e atividades", ["☕️", "🍵", "🍕", "🍔", "🍰", "🎂", "🍻", "🥂", "🎉", "🎊", "🎁", "🎯", "⚽️", "🏆", "🎮", "🎵", "🚀", "🏖️", "💻", "📚", "💡"])
    ]

    private var validEmoji: String? {
        let value = typedEmoji.trimmingCharacters(in: .whitespacesAndNewlines)
        guard value.count == 1,
              value.unicodeScalars.contains(where: {
                  ($0.properties.isEmoji && $0.value > 0x7F) || $0.value == 0x20E3
              })
        else { return nil }
        return value
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Qualquer emoji").font(.headline)
                        Text("Use o teclado de emojis para escolher variações, tons de pele e bandeiras.")
                            .font(.subheadline)
                            .foregroundStyle(Palette.textSecondary)
                        HStack {
                            TextField("Digite ou cole um emoji", text: $typedEmoji)
                                .textFieldStyle(.roundedBorder)
                                .autocorrectionDisabled()
                                .onSubmit { if let emoji = validEmoji { onSelect(emoji) } }
                            Button("Usar") { if let emoji = validEmoji { onSelect(emoji) } }
                                .disabled(validEmoji == nil)
                        }
                        if !typedEmoji.isEmpty && validEmoji == nil {
                            Text("Escolha apenas um emoji.")
                                .font(.caption)
                                .foregroundStyle(Palette.textSecondary)
                        }
                    }
                    ForEach(Self.groups.indices, id: \.self) { index in
                        VStack(alignment: .leading, spacing: 8) {
                            Text(Self.groups[index].title).font(.headline)
                            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 4), count: 7), spacing: 8) {
                                ForEach(Self.groups[index].emojis, id: \.self) { emoji in
                                    Button { onSelect(emoji) } label: {
                                        Text(emoji)
                                            .font(.system(size: 28))
                                            .frame(maxWidth: .infinity, minHeight: 44)
                                            .background {
                                                if emoji == currentEmoji {
                                                    RoundedRectangle(cornerRadius: 12).fill(Palette.action.opacity(0.18))
                                                }
                                            }
                                    }
                                    .buttonStyle(PressScale())
                                    .accessibilityLabel(emoji == currentEmoji ? "Remover \(emoji)" : "Reagir com \(emoji)")
                                }
                            }
                        }
                    }
                }
                .padding()
            }
            .navigationTitle("Escolher reação")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancelar") { dismiss() } }
            }
        }
    }
}

private struct ReactionDetailsSheet: View {
    let spaceId: String
    let entry: TimelineEntry
    let initialEmoji: String
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var selectedEmoji = ""
    @State private var choosingEmoji = false

    private var reaction: ReactionDto? {
        entry.reactions.first { $0.emoji == (selectedEmoji.isEmpty ? initialEmoji : selectedEmoji) }
    }
    private var myReaction: String? { entry.reactions.first(where: \.mine)?.emoji }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 12) {
                            ForEach(entry.reactions, id: \.emoji) { item in
                                Button { selectedEmoji = item.emoji } label: {
                                    Text("\(item.emoji) \(item.people.count)")
                                        .padding(10)
                                        .background(item.emoji == reaction?.emoji ? Palette.action.opacity(0.18) : .clear, in: Capsule())
                                }
                                .buttonStyle(.plain)
                                .accessibilityLabel("\(item.emoji), \(item.people.count) participantes")
                                .accessibilityAddTraits(item.emoji == reaction?.emoji ? .isSelected : [])
                            }
                        }
                    }
                }
                if let reaction {
                    Section("Quem reagiu com \(reaction.emoji)") {
                        ForEach(reaction.people, id: \.id) { person in
                            HStack(spacing: 12) {
                                Avatar(persona: person, size: 36)
                                Text(person.name)
                                if person.isMe {
                                    Text("Você").font(.caption).foregroundStyle(Palette.textSecondary)
                                }
                            }
                        }
                    }
                    Section {
                        Button(reaction.mine ? "Remover minha reação" : "Reagir com \(reaction.emoji)") {
                            react(reaction.mine ? nil : reaction.emoji)
                        }
                        Button(myReaction == nil ? "Escolher outra reação" : "Trocar minha reação") {
                            choosingEmoji = true
                        }
                        if myReaction != nil && !reaction.mine {
                            Button("Remover minha reação", role: .destructive) { react(nil) }
                        }
                    }
                }
            }
            .navigationTitle("Reações")
            .toolbar {
                ToolbarItem(placement: .confirmationAction) { Button("Fechar") { dismiss() } }
            }
            .sheet(isPresented: $choosingEmoji) {
                EmojiReactionPicker(currentEmoji: myReaction) { emoji in
                    react(emoji == myReaction ? nil : emoji)
                }
            }
        }
    }

    private func react(_ emoji: String?) {
        Haptics.selectionTick()
        model.perform { try model.core.react(spaceId: spaceId, messageId: entry.id, emoji: emoji) }
        choosingEmoji = false
        dismiss()
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
