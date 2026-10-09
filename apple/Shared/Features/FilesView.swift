import SwiftUI
import RodaCore
import UniformTypeIdentifiers

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
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    ForEach(model.spaces.prefix(12), id: \.id) { space in
                        Menu(space.title) {
                            Button("New page", systemImage: "doc.badge.plus") { newPage(in: space.id) }
                            Button("Add files", systemImage: "square.and.arrow.down") { importInto = space.id }
                        }
                    }
                } label: {
                    Label("Add", systemImage: "plus")
                }
                .accessibilityIdentifier("files.add")
            }
        }
        .fileImporter(isPresented: Binding(get: { importInto != nil }, set: { if !$0 { importInto = nil } }),
                      allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            guard let space = importInto, case .success(let urls) = result else { return }
            Task { @MainActor in
                let ids = await FileSupport.importFiles(urls, into: space, model: model)
                if ids.count == 1 { model.go(.item(ids[0])) }
                if !ids.isEmpty { Haptics.commit() }
            }
        }
        .task(id: model.revision) { items = model.core.items() }
    }

    @State private var importInto: String?

    private func newPage(in space: String) {
        if let it = model.perform({ try model.core.pageCreate(spaceId: space, title: "") }) {
            Haptics.action()
            model.go(.item(it.id))
        }
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
        if item.kindId == "page" || item.kindId == "file" {
            HStack(spacing: 12) {
                FileThumb(item: item, size: 34)
                VStack(alignment: .leading, spacing: 2) {
                    Text(item.title).font(.body.weight(.medium)).foregroundStyle(Palette.textPrimary).lineLimit(1)
                    Text(subtitle(who: who, at: last?.atMs ?? 0)).font(.caption).foregroundStyle(Palette.textSecondary).lineLimit(1)
                }
                Spacer()
                Image(systemName: "chevron.right").font(.caption.weight(.bold)).foregroundStyle(Palette.textTertiary)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
            .contentShape(.rect)
        } else {
            FileRow(symbol: symbol, tint: tint,
                    title: item.title,
                    subtitle: String(localized: "v\(item.version) · edited by \(who) · \(RodaTime.relative(last?.atMs ?? 0))"))
        }
    }
    private func subtitle(who: String, at: Int64) -> String {
        if let f = item.file {
            let size = ByteCountFormatter.string(fromByteCount: Int64(f.bytes), countStyle: .file)
            return f.ready
                ? String(localized: "v\(item.version) · \(size) · \(who) · \(RodaTime.relative(at))")
                : String(localized: "Arriving… \(f.chunksHere) of \(f.chunks) parts")
        }
        return String(localized: "v\(item.version) · edited by \(who) · \(RodaTime.relative(at))")
    }
    private var symbol: String {
        switch item.kindId {
        case "plan": "map.fill"
        case "task": "checklist"
        case "page": "doc.richtext"
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
    @State private var importing = false
    var body: some View {
        List {
            Section {
                ForEach(items) { it in
                    NavigationLink(value: Route.item(it.id)) { ItemFileRow(item: it) }
                        .listRowInsets(EdgeInsets())
                }
            } footer: {
                Text("Every change is kept as a version. Restoring adds a new version; nothing is lost.")
            }
            Section {
                Button("New page", systemImage: "doc.badge.plus") {
                    if let it = model.perform({ try model.core.pageCreate(spaceId: spaceId, title: "") }) {
                        Haptics.action()
                        model.go(.item(it.id))
                    }
                }
                .accessibilityIdentifier("folder.newPage")
                Button("Add files", systemImage: "square.and.arrow.down") { importing = true }
                Button("Open the chat", systemImage: "bubble.left.and.bubble.right") { model.go(.space(spaceId)) }
            }
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            guard case .success(let urls) = result else { return }
            Task { @MainActor in
                let ids = await FileSupport.importFiles(urls, into: spaceId, model: model)
                if !ids.isEmpty { Haptics.commit() }
            }
        }
        .navigationTitle(model.space(spaceId)?.title ?? String(localized: "Folder"))
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .task(id: model.revision) { items = model.core.items().filter { $0.spaceId == spaceId } }
    }
}
