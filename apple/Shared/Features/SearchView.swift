import SwiftUI
import RodaCore

/// Global search: chats, people and agents, mini-apps, then messages and items (the core's
/// local full-text search). On iPhone it opens as a sheet from the bar's Search button with
/// the field focused; on Mac it is the sidebar's Search destination. Everything is local.
struct SearchScreen: View {
    @Environment(AppModel.self) private var model
    @State private var query = ""
    @State private var hits: [SearchHit] = []
    @FocusState private var focused: Bool
    var focusOnAppear = false
    var onOpenItem: (String) -> Void = { _ in }
    /// Called before navigating away (the sheet closes itself).
    var onLeave: () -> Void = {}

    private var chats: [SpaceSummary] {
        model.spaces.filter { $0.title.localizedCaseInsensitiveContains(query) }.prefix(4).map { $0 }
    }

    private var people: [Persona] {
        var seen = Set<String>()
        return model.spaces.flatMap(\.members)
            .filter { !$0.isMe && $0.name.localizedCaseInsensitiveContains(query) && seen.insert($0.id).inserted }
            .prefix(5).map { $0 }
    }

    private var apps: [ItemDetail] {
        model.liveApps.filter { ($0.app?.name ?? $0.title).localizedCaseInsensitiveContains(query) || $0.title.localizedCaseInsensitiveContains(query) }
    }

    var body: some View {
        List {
            if query.isEmpty {
                Section("Try") {
                    ForEach(AppLocale.isPortuguese ? ["pousada", "Marina", "Financeiro", "TestFlight"] : ["inn", "Marina", "Finance", "TestFlight"], id: \.self) { s in
                        Button { query = s } label: { Label(s, systemImage: "magnifyingglass") }
                    }
                }
            } else if hits.isEmpty && chats.isEmpty && people.isEmpty && apps.isEmpty {
                InkEmptyState(pose: .zen, title: String(localized: "No results for “\(query)”"),
                              message: String(localized: "Check the spelling or try another word."))
                    .listRowBackground(Color.clear)
            } else {
                if !chats.isEmpty {
                    Section("Chats") {
                        ForEach(chats) { s in
                            Button { leave { model.go(.space(s.id)) } } label: {
                                HStack(spacing: 12) {
                                    SpaceAvatar(space: s, size: 34)
                                    VStack(alignment: .leading, spacing: 1) {
                                        Text(s.title).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.textPrimary)
                                        Text(s.lastPreview).font(.caption).foregroundStyle(Palette.textSecondary).lineLimit(1)
                                    }
                                }
                            }
                        }
                    }
                }
                if !people.isEmpty {
                    Section("People and agents") {
                        ForEach(people, id: \.id) { p in
                            Button { leave { open(p) } } label: {
                                HStack(spacing: 12) {
                                    Avatar(persona: p, size: 34)
                                    VStack(alignment: .leading, spacing: 1) {
                                        Text(p.name).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.textPrimary)
                                        Text(p.kind == .agent ? String(localized: "Agent") : "@\(p.handle)")
                                            .font(.caption).foregroundStyle(Palette.textSecondary)
                                    }
                                }
                            }
                        }
                    }
                }
                if !apps.isEmpty {
                    Section("Mini-apps") {
                        ForEach(apps, id: \.id) { item in
                            Button { leave { model.openApp(item.id, fromHome: true) } } label: {
                                HStack(spacing: 12) {
                                    Image(systemName: McpHost.symbol(for: item.app?.appId ?? ""))
                                        .foregroundStyle(McpHost.tint(for: item.app?.appId ?? ""))
                                        .frame(width: 34, height: 34)
                                        .background(McpHost.tint(for: item.app?.appId ?? "").opacity(0.12), in: .rect(cornerRadius: 10, style: .continuous))
                                    VStack(alignment: .leading, spacing: 1) {
                                        Text(item.app?.name ?? item.title).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.textPrimary)
                                        Text(item.spaceTitle).font(.caption).foregroundStyle(Palette.textSecondary)
                                    }
                                }
                            }
                        }
                    }
                }
                if !hits.isEmpty {
                    Section("Messages and items") {
                        ForEach(hits) { h in
                            Button {
                                leave { if let item = h.itemId { onOpenItem(item) } else { model.go(.space(h.spaceId)) } }
                            } label: {
                                HStack(alignment: .top, spacing: 12) {
                                    if h.itemId != nil {
                                        Image(systemName: "map").foregroundStyle(Palette.action).frame(width: 34, height: 34)
                                            .background(Palette.action.opacity(0.12), in: .rect(cornerRadius: 10, style: .continuous))
                                    } else {
                                        Avatar(persona: h.author, size: 34)
                                    }
                                    VStack(alignment: .leading, spacing: 2) {
                                        HStack {
                                            Text(h.title).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.textPrimary)
                                            Spacer()
                                            Text(RodaTime.short(h.atMs)).font(.caption).foregroundStyle(Palette.textTertiary)
                                        }
                                        Text(h.snippet).font(.subheadline).foregroundStyle(Palette.textSecondary).lineLimit(2)
                                        Text(h.spaceTitle).font(.caption).foregroundStyle(Palette.textTertiary)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        .navigationTitle("Search")
        #if os(iOS)
        .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Chats, people, agents, mini-apps")
        #else
        .searchable(text: $query, prompt: "Chats, people, agents, mini-apps")
        #endif
        .searchFocused($focused)
        .onChange(of: query) { _, q in hits = model.core.search(query: q) }
        .task(id: model.revision) { if !query.isEmpty { hits = model.core.search(query: query) } }
        .onAppear {
            if let seed = model.searchSeed { query = seed; model.searchSeed = nil }
            if focusOnAppear { focused = true }
        }
    }

    private func leave(_ go: @escaping () -> Void) {
        onLeave()
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(250))
            go()
        }
    }

    private func open(_ p: Persona) {
        if p.kind == .agent {
            model.push(.agent(p.id))
        } else if let dm = model.spaces.first(where: { $0.counterpart?.id == p.id }) ?? model.spaces.first(where: { $0.members.contains { $0.id == p.id } }) {
            model.go(.space(dm.id))
        }
    }
}
