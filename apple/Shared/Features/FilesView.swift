import SwiftUI
import RodaCore

/// Arquivos (poster #067). No Roda, "arquivo" é um Item: versionado, assinado, com
/// desfazer. Pastas = Espaços onde os Itens nasceram; Recentes = última versão.
struct FilesScreen: View {
    @Environment(AppModel.self) private var model
    @State private var items: [ItemDetail] = []
    @State private var query = ""
    @State private var scope = Scope.all

    enum Scope: String, CaseIterable, Identifiable {
        case all, mine, shared, agents
        var id: String { rawValue }
        var label: String {
            switch self {
            case .all: String(localized: "All")
            case .mine: String(localized: "Personal")
            case .shared: String(localized: "Shared")
            case .agents: String(localized: "From agents")
            }
        }
    }

    private var filtered: [ItemDetail] {
        let q = query.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: .current)
        return items.filter { it in
            let space = model.space(it.spaceId)
            let ok: Bool = switch scope {
            case .all: true
            case .mine: (space?.members.filter { $0.kind == .person }.count ?? 0) <= 1
            case .shared: (space?.members.filter { $0.kind == .person }.count ?? 0) > 1
            case .agents: it.createdBy.kind == .agent
            }
            return ok && (q.isEmpty || "\(it.title) \(it.spaceTitle)".folding(options: [.caseInsensitive, .diacriticInsensitive], locale: .current).contains(q))
        }
    }

    private var folders: [(SpaceSummary, [ItemDetail])] {
        var order: [String] = []
        var map: [String: [ItemDetail]] = [:]
        for it in filtered {
            if map[it.spaceId] == nil { order.append(it.spaceId) }
            map[it.spaceId, default: []].append(it)
        }
        return order.compactMap { id in model.space(id).map { ($0, map[id] ?? []) } }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Label("Only on this device · \(model.stats?.events ?? 0) signed events", systemImage: "iphone")
                    .font(.caption).foregroundStyle(Palette.textSecondary)
                    .padding(.horizontal, 4)
                SearchField(text: $query, prompt: "Search files, chats or people")
                FilterPills(options: Scope.allCases, selection: $scope, label: \.label)

                SectionHeader(title: String(localized: "Folders"), trailing: String(localized: "\(folders.count) folders"))
                VStack(spacing: 0) {
                    ForEach(folders, id: \.0.id) { space, its in
                        NavigationLink(value: Route.folder(space.id)) {
                            FileRow(symbol: "folder.fill", tint: Color(hex: "#F5B83D"),
                                    title: space.title,
                                    subtitle: String(localized: "\(its.count) \(its.count == 1 ? String(localized: "item") : String(localized: "items")) · updated \(RodaTime.relative(its.map(latestAt).max() ?? 0))"))
                        }
                        .buttonStyle(.plain)
                        if space.id != folders.last?.0.id { Divider().padding(.leading, 58).opacity(0.5) }
                    }
                }
                .background(Palette.surface, in: .rect(cornerRadius: 20, style: .continuous))

                SectionHeader(title: String(localized: "Recent"))
                VStack(spacing: 0) {
                    ForEach(filtered) { it in
                        NavigationLink(value: Route.item(it.id)) { ItemFileRow(item: it) }
                            .buttonStyle(.plain)
                        if it.id != filtered.last?.id { Divider().padding(.leading, 58).opacity(0.5) }
                    }
                }
                .background(Palette.surface, in: .rect(cornerRadius: 20, style: .continuous))
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .frame(maxWidth: 720)
            .frame(maxWidth: .infinity)
        }
        .scrollEdgeEffectStyle(.soft, for: .top)
        .background(NightBackdrop())
        .navigationTitle("Files")
        .task(id: model.revision) { items = model.core.items() }
    }
}

func latestAt(_ it: ItemDetail) -> Int64 { it.versions.map(\.atMs).max() ?? 0 }

struct FileRow: View {
    let symbol: String
    let tint: Color
    let title: String
    let subtitle: String
    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: symbol).font(.system(size: 20)).foregroundStyle(tint).frame(width: 34)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.body.weight(.medium)).foregroundStyle(Palette.textPrimary).lineLimit(1)
                Text(subtitle).font(.caption).foregroundStyle(Palette.textSecondary).lineLimit(1)
            }
            Spacer()
            Image(systemName: "chevron.right").font(.caption.weight(.bold)).foregroundStyle(Palette.textTertiary)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .contentShape(.rect)
    }
}

struct ItemFileRow: View {
    let item: ItemDetail
    var body: some View {
        let last = item.versions.max { $0.number < $1.number }
        let who = last.map { $0.author.isMe ? String(localized: "you") : $0.author.name } ?? item.createdBy.name
        FileRow(symbol: symbol, tint: tint,
                title: item.title,
                subtitle: String(localized: "v\(item.version) · edited by \(who) · \(RodaTime.relative(last?.atMs ?? 0))"))
    }
    private var symbol: String {
        switch item.kindId {
        case "plan": "map.fill"
        case "task": "checklist"
        default: "doc.text.fill"
        }
    }
    private var tint: Color {
        item.kindId == "plan" ? Palette.action : (item.kindId == "task" ? Palette.success : Palette.textSecondary)
    }
}

/// Uma "pasta": os Itens de um Espaço.
struct FolderView: View {
    @Environment(AppModel.self) private var model
    let spaceId: String
    @State private var items: [ItemDetail] = []
    var body: some View {
        List {
            Section {
                ForEach(items) { it in
                    NavigationLink(value: Route.item(it.id)) { ItemFileRow(item: it) }
                        .listRowInsets(EdgeInsets())
                }
            } footer: {
                Text("Every version is a signed event in this Space’s log. Restoring creates a new version; nothing is deleted.")
            }
            Section {
                Button("Open the chat", systemImage: "bubble.left.and.bubble.right") { model.go(.space(spaceId)) }
            }
        }
        .navigationTitle(model.space(spaceId)?.title ?? String(localized: "Folder"))
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .task(id: model.revision) { items = model.core.items().filter { $0.spaceId == spaceId } }
    }
}
