import SwiftUI
import RodaCore

// MARK: - Universal search
//
// The bar's search circle opens this full screen (it grows out of the circle). Everything is
// searched on this device: the Rust core keeps a SQLite FTS5 index (prefix matching,
// diacritics folded: "acao" finds "ação") over messages (voice notes by transcript), people,
// agents, chats, items and mini-apps, rebuilt from the signed event log when it changes. Store
// results come from the local seed catalog. Typing is debounced 150 ms.

enum SearchScope: String, CaseIterable, Identifiable {
    case all, messages, people, agents, spaces, apps, files
    var id: String { rawValue }

    var label: String {
        switch self {
        case .all: String(localized: "All")
        case .messages: String(localized: "Messages")
        case .people: String(localized: "People")
        case .agents: String(localized: "Agents")
        case .spaces: String(localized: "Spaces")
        case .apps: String(localized: "Apps")
        case .files: String(localized: "Files")
        }
    }

    /// Core index kinds for this scope.
    var kinds: [String] {
        switch self {
        case .all: []
        case .messages: ["message"]
        case .people: ["person"]
        case .agents: ["agent"]
        case .spaces: ["space"]
        case .apps: ["app"]
        case .files: ["item"]
        }
    }

    static func forKind(_ k: String) -> SearchScope {
        switch k {
        case "message": .messages
        case "person": .people
        case "agent": .agents
        case "space": .spaces
        case "app", "store": .apps
        default: .files
        }
    }
}

struct JumpTarget: Equatable {
    let space: String
    let entry: String
}

/// Results of one query, grouped for display.
struct SearchOutcome {
    var hits: [UniversalHit] = []
    var store: [StoreListing] = []
    var tookUs: UInt64 = 0
    var isEmpty: Bool { hits.isEmpty && store.isEmpty }
}

@MainActor
enum SearchEngine {
    static func run(_ q: String, scope: SearchScope, model: AppModel) -> SearchOutcome {
        let query = q.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return SearchOutcome() }
        var out = SearchOutcome()
        let limit: UInt32 = scope == .all ? 4 : 40
        if let r = try? model.core.universalSearch(query: query, kinds: scope.kinds, limit: limit) {
            out.hits = r.hits
            out.tookUs = r.tookUs
        }
        if scope == .all || scope == .apps {
            let terms = fold(query).split(separator: " ").map(String.init)
            out.store = StoreSeed.listings.filter { l in
                let hay = fold([l.name, l.subtitle ?? "", l.blurb, l.creator].joined(separator: " "))
                let words = hay.split(whereSeparator: { !$0.isLetter && !$0.isNumber }).map(String.init)
                return terms.allSatisfy { t in words.contains { $0.hasPrefix(t) } }
            }
            if scope == .all { out.store = Array(out.store.prefix(3)) }
        }
        return out
    }

    static func fold(_ s: String) -> String {
        s.folding(options: [.diacriticInsensitive, .caseInsensitive], locale: nil)
    }

    /// "[[hike]] on Saturday" → bold, primary-coloured matches.
    static func highlight(_ marked: String, base: Color = Palette.textSecondary) -> AttributedString {
        var out = AttributedString()
        var rest = Substring(marked)
        while let open = rest.range(of: "[[") {
            var plain = AttributedString(String(rest[..<open.lowerBound]))
            plain.foregroundColor = base
            out += plain
            rest = rest[open.upperBound...]
            guard let close = rest.range(of: "]]") else { break }
            var bold = AttributedString(String(rest[..<close.lowerBound]))
            bold.inlinePresentationIntent = .stronglyEmphasized
            bold.foregroundColor = Palette.textPrimary
            out += bold
            rest = rest[close.upperBound...]
        }
        var tail = AttributedString(String(rest))
        tail.foregroundColor = base
        out += tail
        return out
    }
}

/// Recent queries (this device only).
enum SearchRecents {
    static let key = "RodaRecentSearches"
    static var all: [String] { UserDefaults.standard.stringArray(forKey: key) ?? [] }
    static func add(_ q: String) {
        let t = q.trimmingCharacters(in: .whitespaces)
        guard t.count > 1 else { return }
        var list = all.filter { $0.caseInsensitiveCompare(t) != .orderedSame }
        list.insert(t, at: 0)
        UserDefaults.standard.set(Array(list.prefix(8)), forKey: key)
    }
    static func clear() { UserDefaults.standard.removeObject(forKey: key) }
}

struct UniversalSearchView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.colorScheme) private var scheme
    var embedded = false
    var onClose: () -> Void = {}

    @State private var query = ""
    @State private var scope: SearchScope = .all
    @State private var outcome = SearchOutcome()
    @State private var searched = ""
    @State private var recents = SearchRecents.all
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            field
                .padding(.horizontal, 16)
            chips
                .padding(.top, 12)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 18, pinnedViews: []) {
                    if query.trimmingCharacters(in: .whitespaces).isEmpty {
                        emptyState
                    } else if outcome.isEmpty && searched == query {
                        noResults
                    } else {
                        results
                    }
                    if UserDefaults.standard.bool(forKey: "RodaSearchTiming"), !query.isEmpty {
                        Text(verbatim: "\(outcome.hits.count + outcome.store.count) results · \(String(format: "%.1f", Double(outcome.tookUs) / 1000)) ms on device")
                            .font(.caption2.monospacedDigit())
                            .foregroundStyle(Palette.textTertiary)
                            .frame(maxWidth: .infinity)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.top, 16)
                .padding(.bottom, 40)
            }
            .scrollDismissesKeyboard(.interactively)
        }
        .background(Palette.background.ignoresSafeArea())
        .task(id: "\(query)|\(scope.rawValue)") {
            // Debounce typing by 150 ms.
            if !query.isEmpty { try? await Task.sleep(for: .milliseconds(150)) }
            guard !Task.isCancelled else { return }
            outcome = SearchEngine.run(query, scope: scope, model: model)
            searched = query
        }
        .task {
            if let seed = model.searchSeed { query = seed; model.searchSeed = nil }
            if let s = UserDefaults.standard.string(forKey: "RodaSearchScope"), let sc = SearchScope(rawValue: s) { scope = sc }
            try? await Task.sleep(for: .milliseconds(250))
            if !UserDefaults.standard.bool(forKey: "RodaSearchNoFocus") { focused = true }
            // Video: `-RodaSearchType elk` types the query letter by letter.
            if let typed = UserDefaults.standard.string(forKey: "RodaSearchType") {
                try? await Task.sleep(for: .seconds(0.6))
                for ch in typed { query.append(ch); try? await Task.sleep(for: .milliseconds(220)) }
                try? await Task.sleep(for: .milliseconds(400))
                focused = false
            }
            // Video: `-RodaSearchJump YES` opens the first message hit after a beat.
            guard UserDefaults.standard.bool(forKey: "RodaSearchJump") else { return }
            try? await Task.sleep(for: .seconds(2.2))
            if let h = outcome.hits.first(where: { $0.kind == "message" }) { open(h) }
        }
    }

    // MARK: Header, field, chips

    private var header: some View {
        HStack(alignment: .firstTextBaseline) {
            Text("Search")
                .font(.largeTitle.weight(.bold))
                .foregroundStyle(Palette.textPrimary)
                .accessibilityAddTraits(.isHeader)
            Spacer()
            if !embedded {
                Button {
                    Haptics.dismiss()
                    focused = false
                    onClose()
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 16, weight: .semibold))
                        .foregroundStyle(Palette.textPrimary)
                        .frame(width: 44, height: 44)
                        .glassEffect(.regular.interactive(), in: .circle)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Close search"))
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 8)
        .padding(.bottom, 10)
    }

    private var field: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 17, weight: .medium))
                .foregroundStyle(Palette.textSecondary)
            TextField(String(localized: "Messages, people, agents, apps…"), text: $query)
                .textFieldStyle(.plain)
                .font(.body)
                .autocorrectionDisabled()
                #if os(iOS)
                .textInputAutocapitalization(.never)
                #endif
                .submitLabel(.search)
                .focused($focused)
                .onSubmit { SearchRecents.add(query); recents = SearchRecents.all }
                .accessibilityIdentifier("search-field")
            if !query.isEmpty {
                Button {
                    Haptics.selectionTick()
                    query = ""
                    focused = true
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.system(size: 17))
                        .foregroundStyle(Palette.textTertiary)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Clear search"))
                .transition(.scale.combined(with: .opacity))
            }
        }
        .padding(.horizontal, 16)
        .frame(minHeight: 50)
        .glassEffect(.regular.interactive(), in: .capsule)
        .animation(.snappy, value: query.isEmpty)
    }

    private var chips: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(SearchScope.allCases) { s in
                    let on = scope == s
                    Button {
                        Haptics.selectionTick()
                        withAnimation(.snappy) { scope = s }
                    } label: {
                        Text(s.label)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(on ? Color.white : Palette.textPrimary)
                            .padding(.horizontal, 16)
                            .frame(minHeight: 36)
                            .background { if on { Capsule().fill(Palette.action) } }
                            .glassEffect(on ? .identity : .regular.interactive(), in: .capsule)
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(on ? .isSelected : [])
                    .accessibilityIdentifier("scope-\(s.rawValue)")
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 2)
        }
        .scrollClipDisabled()
    }

    // MARK: States

    @ViewBuilder private var emptyState: some View {
        if !recents.isEmpty {
            section(String(localized: "Recent"), action: (String(localized: "Clear"), { SearchRecents.clear(); recents = [] })) {
                VStack(spacing: 0) {
                    ForEach(recents, id: \.self) { r in
                        Button { query = r } label: {
                            HStack(spacing: 12) {
                                Image(systemName: "clock.arrow.circlepath").foregroundStyle(Palette.textTertiary)
                                Text(r).foregroundStyle(Palette.textPrimary)
                                Spacer()
                                Image(systemName: "arrow.up.left").font(.caption).foregroundStyle(Palette.textTertiary)
                            }
                            .padding(.vertical, 10)
                            .contentShape(.rect)
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, 14)
                .background(card)
            }
        }
        section(String(localized: "Suggested")) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(alignment: .top, spacing: 14) {
                    ForEach(suggestedPeople, id: \.id) { p in
                        Button { open(person: p) } label: {
                            VStack(spacing: 6) {
                                ContactAvatar(persona: p, size: 56)
                                Text(p.name.split(separator: " ").first.map(String.init) ?? p.name)
                                    .font(.caption.weight(.medium))
                                    .foregroundStyle(Palette.textPrimary)
                                    .lineLimit(1)
                            }
                            .frame(width: 66)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(Text(p.name))
                    }
                }
                .padding(.vertical, 2)
            }
            .scrollClipDisabled()
        }
        section(String(localized: "Apps for you")) {
            VStack(spacing: 0) {
                ForEach(StoreSeed.featured.prefix(3).compactMap(StoreSeed.listing), id: \.id) { l in
                    storeRow(l)
                }
            }
            .padding(.horizontal, 14)
            .background(card)
        }
    }

    @ViewBuilder private var noResults: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Didn’t find what you need?")
                .font(.headline)
                .foregroundStyle(Palette.textPrimary)
            VStack(alignment: .leading, spacing: 0) {
                HStack(alignment: .top, spacing: 12) {
                    Image(systemName: "magnifyingglass").foregroundStyle(Palette.textTertiary).frame(width: 36, height: 36)
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Try another word, or one of these:")
                            .font(.subheadline)
                            .foregroundStyle(Palette.textSecondary)
                        HStack(spacing: 8) {
                            ForEach(suggestions, id: \.self) { s in
                                Button { query = s } label: {
                                    Text(s).font(.subheadline.weight(.medium))
                                        .foregroundStyle(Palette.textPrimary)
                                        .padding(.horizontal, 12).padding(.vertical, 6)
                                        .background(Palette.textPrimary.opacity(0.06), in: .capsule)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
                .padding(.vertical, 12)
                Divider().padding(.leading, 48)
                Button(action: askZoen) {
                    HStack(spacing: 12) {
                        Image(systemName: "sparkles")
                            .font(.system(size: 16, weight: .semibold))
                            .foregroundStyle(.white)
                            .frame(width: 36, height: 36)
                            .background(Palette.action, in: .circle)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Ask Zoen").font(.body.weight(.semibold)).foregroundStyle(Palette.textPrimary)
                            Text("“\(query)”").font(.subheadline).foregroundStyle(Palette.textSecondary).lineLimit(1)
                        }
                        Spacer()
                        Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(Palette.textTertiary)
                    }
                    .padding(.vertical, 12)
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("ask-zoen")
            }
            .padding(.horizontal, 14)
            .background(card)
        }
    }

    @ViewBuilder private var results: some View {
        let order: [(SearchScope, String)] = [(.messages, "message"), (.people, "person"), (.agents, "agent"),
                                              (.spaces, "space"), (.apps, "app"), (.files, "item")]
        ForEach(order, id: \.1) { sc, kind in
            let hits = outcome.hits.filter { $0.kind == kind }
            let store = kind == "app" ? outcome.store : []
            if !hits.isEmpty || !store.isEmpty {
                section(sc.label, action: scope == .all ? (String(localized: "See all"), { withAnimation(.snappy) { scope = sc } }) : nil) {
                    VStack(spacing: 0) {
                        ForEach(Array(hits.enumerated()), id: \.element.refId) { i, h in
                            if i > 0 { Divider().padding(.leading, 62) }
                            hitRow(h)
                        }
                        ForEach(Array(store.enumerated()), id: \.element.id) { i, l in
                            if i > 0 || !hits.isEmpty { Divider().padding(.leading, 62) }
                            storeRow(l)
                        }
                    }
                    .padding(.horizontal, 14)
                    .background(card)
                }
            }
        }
    }

    // MARK: Rows

    private func hitRow(_ h: UniversalHit) -> some View {
        Button { open(h) } label: {
            HStack(alignment: .top, spacing: 12) {
                hitIcon(h)
                VStack(alignment: .leading, spacing: 3) {
                    HStack(alignment: .firstTextBaseline) {
                        Text(SearchEngine.highlight(h.titleSnippet.isEmpty ? h.title : h.titleSnippet, base: Palette.textPrimary))
                            .font(.body.weight(.semibold))
                            .lineLimit(1)
                        Spacer(minLength: 6)
                        if h.atMs > 0 && (h.kind == "message" || h.kind == "item" || h.kind == "app") {
                            Text(RodaTime.short(h.atMs))
                                .font(.caption)
                                .foregroundStyle(Palette.textTertiary)
                        }
                    }
                    if let line = subtitle(h) {
                        Text(line)
                            .font(.subheadline)
                            .lineLimit(2)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    if let ctx = h.spaceTitle, h.kind != "space" {
                        HStack(spacing: 4) {
                            Image(systemName: "person.2.fill").font(.system(size: 9))
                            Text(ctx).lineLimit(1)
                        }
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(Palette.textSecondary)
                        .padding(.horizontal, 8).padding(.vertical, 3)
                        .background(Palette.textPrimary.opacity(0.06), in: .capsule)
                        .padding(.top, 2)
                    }
                }
            }
            .padding(.vertical, 12)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityHint(Text(h.kind == "message" ? "Opens the chat at this message" : "Opens it"))
    }

    private func subtitle(_ h: UniversalHit) -> AttributedString? {
        switch h.kind {
        case "person", "agent":
            let bio = h.persona?.bio ?? ""
            if h.snippet.contains("[[") { return SearchEngine.highlight(h.snippet) }
            return bio.isEmpty ? AttributedString("@\(h.persona?.handle ?? "")") : SearchEngine.highlight(bio)
        case "space":
            return SearchEngine.highlight(h.snippet)
        default:
            return h.snippet.isEmpty ? nil : SearchEngine.highlight(h.snippet)
        }
    }

    @ViewBuilder private func hitIcon(_ h: UniversalHit) -> some View {
        switch h.kind {
        case "message", "person", "agent":
            if let p = h.persona { ContactAvatar(persona: p, size: 40) } else { iconTile("bubble.left.fill", tint: Palette.action) }
        case "space":
            if let s = model.space(h.refId), let c = s.counterpart { ContactAvatar(persona: c, size: 40) }
            else { iconTile("person.3.fill", tint: Palette.action) }
        case "app":
            iconTile("square.grid.2x2.fill", tint: Color(hex: "#C25B2A"))
        default:
            iconTile("doc.text.fill", tint: Color(hex: "#3B82C4"))
        }
    }

    private func iconTile(_ symbol: String, tint: Color) -> some View {
        Image(systemName: symbol)
            .font(.system(size: 17, weight: .semibold))
            .foregroundStyle(tint)
            .frame(width: 40, height: 40)
            .background(tint.opacity(0.14), in: .rect(cornerRadius: 11, style: .continuous))
    }

    private func storeRow(_ l: StoreListing) -> some View {
        Button { open(listing: l) } label: {
            HStack(alignment: .top, spacing: 12) {
                ListingIcon(listing: l, size: 40)
                VStack(alignment: .leading, spacing: 3) {
                    HStack(alignment: .firstTextBaseline) {
                        Text(l.name).font(.body.weight(.semibold)).foregroundStyle(Palette.textPrimary).lineLimit(1)
                        Spacer(minLength: 6)
                        Text(l.isAgent ? String(localized: "Agent") : String(localized: "Mini-app"))
                            .font(.caption).foregroundStyle(Palette.textTertiary)
                    }
                    Text(l.blurb).font(.subheadline).foregroundStyle(Palette.textSecondary).lineLimit(2)
                    HStack(spacing: 4) {
                        Image(systemName: "bag.fill").font(.system(size: 9))
                        Text("Store · @\(l.creator)")
                    }
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(Palette.textSecondary)
                    .padding(.horizontal, 8).padding(.vertical, 3)
                    .background(Palette.textPrimary.opacity(0.06), in: .capsule)
                    .padding(.top, 2)
                }
            }
            .padding(.vertical, 12)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
    }

    private var card: some View {
        RoundedRectangle(cornerRadius: 20, style: .continuous)
            .fill(scheme == .dark ? Color.white.opacity(0.06) : Color.white.opacity(0.9))
            .shadow(color: .black.opacity(scheme == .dark ? 0 : 0.04), radius: 6, y: 2)
    }

    private func section<C: View>(_ title: String, action: (String, () -> Void)? = nil, @ViewBuilder _ content: () -> C) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline) {
                Text(title).font(.headline).foregroundStyle(Palette.textPrimary).accessibilityAddTraits(.isHeader)
                Spacer()
                if let action {
                    Button(action.0, action: action.1)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(Palette.action)
                        .buttonStyle(.plain)
                }
            }
            content()
        }
    }

    // MARK: Data

    private var suggestedPeople: [Persona] {
        var seen = Set<String>()
        var out: [Persona] = []
        for s in model.spaces {
            for m in s.members where !m.isMe && seen.insert(m.id).inserted { out.append(m) }
        }
        let agents = out.filter { $0.kind == .agent }, people = out.filter { $0.kind == .person }
        return Array((people.prefix(4) + agents.prefix(4)))
    }

    private var suggestions: [String] {
        AppLocale.isPortuguese ? ["trilha", "Paraty", "Marina"] : ["hike", "Paraty", "Marina"]
    }

    // MARK: Navigation

    private func leave() {
        SearchRecents.add(query)
        recents = SearchRecents.all
        focused = false
        onClose()
    }

    private func open(_ h: UniversalHit) {
        Haptics.tap()
        leave()
        switch h.kind {
        case "message":
            guard let sid = h.spaceId else { return }
            model.jumpTarget = JumpTarget(space: sid, entry: h.refId)
            model.go(.space(sid))
        case "person":
            if let p = h.persona { open(person: p, alreadyLeft: true) }
        case "agent":
            model.push(.agent(h.refId))
        case "space":
            model.go(.space(h.refId))
        case "app":
            model.openApp(h.refId)
        default:
            model.go(.item(h.refId))
        }
    }

    private func open(person p: Persona, alreadyLeft: Bool = false) {
        if !alreadyLeft { Haptics.tap(); leave() }
        if p.kind == .agent { model.push(.agent(p.id)); return }
        if let dm = model.spaces.first(where: { $0.counterpart?.id == p.id }) ?? model.spaces.first(where: { $0.members.contains { $0.id == p.id } }) {
            model.go(.space(dm.id))
        }
    }

    private func open(listing l: StoreListing) {
        Haptics.tap()
        leave()
        model.push(.store(l.id))
    }

    private func askZoen() {
        let q = query
        guard let id = model.zoenSpaceId(), !q.isEmpty else { return }
        Haptics.action()
        leave()
        model.go(.space(id))
        Task { await model.send(q, in: id) }
    }
}

/// The search screen grows out of the bar's search circle (a circular reveal anchored on it)
/// and shrinks back into it on close.
struct SearchReveal: View {
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let origin: CGPoint
    @State private var open = false
    /// Once the reveal has finished the mask is dropped entirely.
    @State private var settled = false

    var body: some View {
        GeometryReader { g in
            let full = hypot(g.size.width, g.size.height) * 2.2
            UniversalSearchView {
                close()
            }
            .mask {
                if settled {
                    Rectangle().ignoresSafeArea()
                } else {
                    let gf = g.frame(in: .global)
                    Circle()
                        .frame(width: open ? full : 52, height: open ? full : 52)
                        .position(x: origin.x - gf.minX, y: origin.y - gf.minY)
                }
            }
            .opacity(reduceMotion ? (open ? 1 : 0) : 1)
        }
        .onAppear {
            withAnimation(reduceMotion ? .easeOut(duration: 0.2) : .spring(duration: 0.5, bounce: 0.12)) { open = true } completion: {
                settled = true
            }
        }
    }

    private func close() {
        settled = false
        withAnimation(reduceMotion ? .easeIn(duration: 0.15) : .spring(duration: 0.38, bounce: 0)) { open = false } completion: {
            model.searchOpen = false
        }
    }
}
