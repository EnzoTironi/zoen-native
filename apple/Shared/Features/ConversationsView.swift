import SwiftUI
import RodaCore

enum ChatInboxFilter: String, CaseIterable, Identifiable {
    case all, direct, groups, communities
    var id: Self { self }

    var title: String {
        switch self {
        case .all: String(localized: "All chats")
        case .direct: String(localized: "Direct chats")
        case .groups: String(localized: "Groups")
        case .communities: String(localized: "Communities")
        }
    }

    func includes(_ space: SpaceSummary) -> Bool {
        switch self {
        case .all: true
        case .direct: space.kind == .direct
        case .groups: space.kind == .group
        case .communities: space.kind == .community
        }
    }
}

struct ChatInboxFilterMenu: View {
    @Environment(AppModel.self) private var model
    var showsLabel = true

    var body: some View {
        @Bindable var model = model
        Menu {
            Picker("Filter chats", selection: $model.chatFilter) {
                ForEach(ChatInboxFilter.allCases) { filter in
                    Text(filter.title).tag(filter)
                        .accessibilityIdentifier("chat-filter-\(filter.rawValue)")
                }
            }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "line.3.horizontal.decrease")
                if showsLabel { Text(model.chatFilter.title) }
            }
            .font(.subheadline.weight(.medium))
            .foregroundStyle(model.chatFilter == .all ? Palette.textSecondary : Palette.action)
            .frame(minWidth: 28, minHeight: showsLabel ? 44 : 28)
        }
        #if os(macOS)
        .menuStyle(.borderlessButton)
        #endif
        .accessibilityLabel("Filter chats")
        .accessibilityValue(model.chatFilter.title)
        .accessibilityIdentifier("chat-filter")
    }
}

/// Every conversation shares one inbox, pinned first and then by recency.
struct ConversationsList: View {
    @Environment(AppModel.self) private var model
    var query: String = ""
    var onOpen: (String) -> Void
    var selected: String? = nil

    private var filtered: [SpaceSummary] {
        let q = query.trimmingCharacters(in: .whitespaces).folding(options: [.caseInsensitive, .diacriticInsensitive], locale: .current)
        let inbox = model.orderedSpaces.filter { model.chatFilter.includes($0) }
        guard !q.isEmpty else { return inbox }
        return inbox.filter { s in
            ([s.title, s.lastPreview] + s.members.map(\.name)).joined(separator: " ")
                .folding(options: [.caseInsensitive, .diacriticInsensitive], locale: .current)
                .contains(q)
        }
    }

    var body: some View {
        LazyVStack(alignment: .leading, spacing: 2) {
            ForEach(filtered) { space in
                Button {
                    onOpen(space.id)
                } label: {
                    ConversationRow(space: space, pinned: model.isPinned(space), working: model.working[space.id] != nil, selected: selected == space.id)
                }
                .buttonStyle(.plain)
                .contextMenu {
                    Button {
                        withAnimation(.spring(duration: 0.4)) { model.togglePin(space) }
                    } label: { Label { Text(model.isPinned(space) ? "Unpin" : "Pin") } icon: { ZoenGlyph.pin.menuImage } }
                    Button { model.perform { try model.core.markRead(spaceId: space.id) } } label: {
                        Label { Text("Mark as read") } icon: { ZoenGlyph.check.menuImage }
                    }
                }
            }
            if filtered.isEmpty && !query.isEmpty {
                InkEmptyState(pose: .map, title: String(localized: "Nothing for “\(query)”"))
            } else if filtered.isEmpty && model.chatFilter != .all {
                InkEmptyState(pose: .map, title: String(localized: "No chats in this filter"))
            }
        }
    }
}

struct ConversationRow: View {
    let space: SpaceSummary
    var pinned = false
    var working = false
    var selected = false

    var body: some View {
        let unread = space.unread > 0
        HStack(alignment: .center, spacing: 14) {
            ChatAvatar(space: space, size: 56, working: working)
            VStack(alignment: .leading, spacing: 3) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(space.title)
                        .font(.body.weight(unread ? .bold : .medium))
                        .foregroundStyle(Palette.textPrimary)
                        .lineLimit(1)
                    Spacer(minLength: 6)
                    Text(RodaTime.short(space.lastAtMs))
                        .font(.footnote)
                        .foregroundStyle(Palette.textTertiary)
                }
                HStack(alignment: .center, spacing: 6) {
                    Group {
                        if working, let agent = space.members.first(where: { $0.kind == .agent && $0.isMine }) {
                            Text("\(agent.name) is typing…").foregroundStyle(Palette.action)
                        } else {
                            Text("\(authorPrefix)\(VoiceNoteRef.preview(space.lastPreview) ?? ChatBackgroundMarker.preview(space.lastPreview) ?? space.lastPreview)")
                                .foregroundStyle(unread ? Palette.textPrimary.opacity(0.8) : Palette.textSecondary)
                        }
                    }
                    .font(.subheadline)
                    .lineLimit(1)
                    Spacer(minLength: 6)
                    if pinned {
                        ZoenIcon(.pin, size: 13)
                            .foregroundStyle(Palette.textTertiary)
                    }
                    if unread {
                        Circle().fill(Palette.action).frame(width: 10, height: 10)
                            .transition(.scale.combined(with: .opacity))
                    }
                }
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 10)
        .background(selected ? Palette.action.opacity(0.10) : .clear, in: .rect(cornerRadius: 16, style: .continuous))
        .contentShape(.rect)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilityText)
    }

    private var authorPrefix: String {
        guard let a = space.lastAuthor else { return "" }
        if a.isMe { return String(localized: "You: ") }
        if space.kind == .direct { return "" }
        return "\(a.name): "
    }

    /// The labels the row no longer shows still reach VoiceOver.
    private var accessibilityText: String {
        var parts = [space.title]
        if let c = space.counterpart, c.kind == .agent { parts.append(c.isMine ? String(localized: "your agent") : String(localized: "Agent")) }
        else if space.kind == .group { parts.append(String(localized: "Group")) }
        else if space.kind == .community { parts.append(String(localized: "Community")) }
        if pinned { parts.append(String(localized: "Pinned")) }
        if space.unread > 0 { parts.append(String(localized: "\(space.unread) unread")) }
        parts.append(authorPrefix + space.lastPreview)
        parts.append(RodaTime.short(space.lastAtMs))
        return parts.joined(separator: ", ")
    }
}

/// Chat avatar: anyone 1:1 gets their round face (agents included), and groups, spaces and
/// communities a glossy sphere in the chat's own colour.
struct ChatAvatar: View {
    let space: SpaceSummary
    var size: CGFloat = 56
    var working = false

    var body: some View {
        Group {
            if let c = space.counterpart {
                // Agents are contacts: the same round face as a person, no tile or owner badge.
                ContactAvatar(persona: c, size: size)
            } else {
                // Groups / Spaces: hand-drawn doodle art (still fallback for lists & Reduce Motion).
                GroupAvatar(space: space, size: size)
            }
        }
        .frame(width: size, height: size)
    }
}

/// A glossy sphere (Wabi-style), lit from the top left. The colour comes from a stable hash
/// of the name, from the app's own palette.
struct GlossySphere: View {
    let seed: String
    var size: CGFloat = 56

    private static let palettes: [(Color, Color)] = [
        (Color(hex: "#A8E07A"), Color(hex: "#3D7A28")),   // moss
        (Color(hex: "#FFC3A6"), Color(hex: "#E2563B")),   // coral
        (Color(hex: "#B8DCFF"), Color(hex: "#3D7CD6")),   // sky
        (Color(hex: "#FFE49A"), Color(hex: "#E09A1D")),   // butter
        (Color(hex: "#DCCEFF"), Color(hex: "#7A5AD8")),   // lilac
        (Color(hex: "#A6ECD8"), Color(hex: "#219A7A")),   // mint
    ]

    var body: some View {
        let h = seed.unicodeScalars.reduce(UInt32(5381)) { ($0 &* 33) &+ $1.value }
        let (light, deep) = Self.palettes[Int(h % UInt32(Self.palettes.count))]
        ZStack {
            Circle().fill(RadialGradient(colors: [light, deep], center: UnitPoint(x: 0.32, y: 0.28), startRadius: size * 0.02, endRadius: size * 0.72))
            // Rim light at the bottom and a soft specular highlight at the top left.
            Circle().strokeBorder(LinearGradient(colors: [.clear, .white.opacity(0.35)], startPoint: .top, endPoint: .bottom), lineWidth: size * 0.03)
            Ellipse()
                .fill(RadialGradient(colors: [.white.opacity(0.95), .white.opacity(0)], center: .center, startRadius: 0, endRadius: size * 0.22))
                .frame(width: size * 0.42, height: size * 0.3)
                .offset(x: -size * 0.14, y: -size * 0.2)
        }
        .frame(width: size, height: size)
        .shadow(color: deep.opacity(0.28), radius: size * 0.08, y: size * 0.05)
        .accessibilityHidden(true)
    }
}

/// Home (iPhone): the zoen wordmark, the live mini-app strip, then your chats.
struct ConversationsScreen: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let searching = model.chatSearch || !model.chatQuery.isEmpty
        VStack(spacing: 0) {
            // Glass chrome stays put; the refresh band grows beneath it (Snapchat-style).
            if !searching {
                HomeHeader()
                    .padding(.horizontal, 20)
                    .padding(.top, 4)
                    .padding(.bottom, 8)
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    if !searching {
                        HomeStrip()
                            .transition(.opacity.combined(with: .move(edge: .top)))
                    }
                    ChatInboxFilterMenu()
                        .padding(.horizontal, 20)
                    ConversationsList(query: model.chatQuery) { id in model.push(.space(id)) }
                }
                .padding(.top, searching ? 12 : 0)
                .padding(.bottom, 110)
                .animation(.spring(duration: 0.4, bounce: 0.15), value: searching)
            }
            .scrollDismissesKeyboard(.interactively)
            .scrollEdgeEffectStyle(.soft, for: .top)
            .defaultScrollAnchor(UserDefaults.standard.bool(forKey: "RodaScrollChats") ? .bottom : .top)
        }
        .background(NightBackdrop())
        .navigationTitle("Chats")
        #if os(iOS)
        .toolbar(.hidden, for: .navigationBar)
        #endif
    }
}

/// "zoen" wordmark with Zo's head on the left; the notifications bell (approvals live there)
/// and your avatar on the right.
struct HomeHeader: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        HStack(spacing: 10) {
            HStack(spacing: 6) {
                MascotHead(size: 30, mood: .smirk)
                Text(verbatim: "zoen")
                    .font(.system(size: 30, weight: .heavy, design: .rounded))
                    .foregroundStyle(Palette.textPrimary)
                    .kerning(-0.8)
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Zoen")
            .accessibilityAddTraits(.isHeader)
            Spacer()
            if model.sync.account != nil || SyncModel.mode == .demo {
                ConnectionDot()
                Button {
                    Haptics.tap()
                    model.newChatOpen = true
                } label: {
                    Image(systemName: "square.and.pencil")
                        .font(.system(size: 19, weight: .semibold))
                        .foregroundStyle(Palette.textPrimary)
                        .frame(width: 40, height: 40)
                        .glassEffect(.regular.interactive(), in: .circle)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("New chat")
                .accessibilityIdentifier("newChat")
            }
            NotificationsBell()
            if let me = model.me {
                Button {
                    Haptics.tap()
                    model.select(.you)
                } label: {
                    Avatar(persona: me, size: 40)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Your context")
            }
        }
    }
}

/// The live mini-app cards (the same snapshot the widgets show). Tap opens; long-press to
/// move or unpin.
struct HomeStrip: View {
    @Environment(AppModel.self) private var model
    @Environment(\.appZoom) private var zoom
    static let side: CGFloat = 170
    /// Jiggle edit mode (long-press a card): reorder by dragging, minus to unpin.
    @State private var editing = false

    private struct Card: Identifiable { let id: String; let item: ItemDetail; let snap: WidgetSnapshot }

    var body: some View {
        let cards = model.homeApps.compactMap { item in WidgetSnapshot.from(item).map { Card(id: item.id, item: item, snap: $0) } }
        if cards.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) { PinHereCard(side: Self.side) }
                .contentMargins(.horizontal, 20, for: .scrollContent)
                .scrollClipDisabled()
                .frame(height: Self.side)
        } else {
            EditableTileStrip(
                items: cards, tileWidth: Self.side, spacing: 12, margin: 20, idPrefix: "home-tile",
                editing: $editing,
                title: { $0.snap.title },
                open: { c in Haptics.open(); model.openApp(c.id, fromHome: true) },
                openFrom: { c, frame, front in model.flipOpenApp(c.id, from: frame, sourceKey: "home-tile-\(c.id)", front: front) },
                move: { ids in model.setHomeOrder(ids) },
                remove: { c in model.unpinFromHome(c.id) },
                removeTitle: { c in String(localized: "Unpin “\(c.snap.title)” from Home?") },
                removeMessage: "It stays in its chat; you can pin it again from there.",
                removeAction: "Unpin from Home"
            ) { c in
                SnapshotCard(snap: c.snap, side: Self.side)
                    .appZoomSource(c.id, zoom)
            }
            .frame(minHeight: Self.side)
        }
    }
}

/// A gentle press-in for cards.
struct PressScaleStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.96 : 1)
            .animation(.spring(duration: 0.25, bounce: 0.3), value: configuration.isPressed)
    }
}

/// Campo de busca em cápsula.
struct SearchField: View {
    @Binding var text: String
    var prompt: LocalizedStringKey
    var body: some View {
        HStack(spacing: 8) {
            ZoenIcon(.search, size: 17).foregroundStyle(Palette.textTertiary)
            TextField(prompt, text: $text)
                .textFieldStyle(.plain)
                .autocorrectionDisabled()
            if !text.isEmpty {
                Button { text = "" } label: { ZoenIcon(.close, size: 14).foregroundStyle(Palette.textTertiary) }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Clear search")
            }
        }
        .font(.body)
        .padding(.horizontal, 12)
        .frame(height: 40)
        .background(Palette.surfaceMuted.opacity(0.85), in: .rect(cornerRadius: 12, style: .continuous))
    }
}

/// Shown only when the relay isn't reachable (or still connecting): chats keep working
/// on this device and go out when it's back.
struct ConnectionDot: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let c = model.sync.connection
        if c.state != "online" {
            HStack(spacing: 5) {
                Circle().fill(c.state == "connecting" ? Color.orange : Palette.textTertiary).frame(width: 7, height: 7)
                Text(c.state == "connecting" ? String(localized: "Connecting") : (c.pending > 0 ? String(localized: "Offline · \(c.pending) queued") : String(localized: "Offline")))
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(Palette.textSecondary)
            }
            .padding(.horizontal, 10).padding(.vertical, 6)
            .glassEffect(.regular, in: .capsule)
            .help(c.error ?? "")
            .accessibilityElement(children: .combine)
            .transition(.opacity.combined(with: .scale(scale: 0.9)))
        }
    }
}
