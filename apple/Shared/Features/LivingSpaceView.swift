import SwiftUI
import RodaCore

/// Opt-in only until the living-space projection is available on every client.
struct SpaceView: View {
    @Environment(AppModel.self) private var model
    @AppStorage("LivingSpacesEnabled") private var enabled = false
    let spaceId: String
    var onOpenItem: (String) -> Void
    private var kind: SpaceKindDto? { model.space(spaceId)?.kind ?? (try? model.core.space(spaceId: spaceId))?.kind }

    var body: some View {
        if enabled, kind == .community {
            LivingSpaceHome(spaceId: spaceId)
        } else if enabled, kind == .group {
            LivingSpaceConversation(spaceId: spaceId)
        } else {
            ConversationSpaceView(spaceId: spaceId, onOpenItem: onOpenItem)
        }
    }
}

/// Canonical compact layout. Wide clients retain their existing sidebar/detail shell.
struct LivingSpaceHome: View {
    @Environment(AppModel.self) private var model
    @Environment(\.colorScheme) private var colorScheme
    let spaceId: String
    @State private var pages: [ItemDetail] = []
    @State private var pageId: String?
    @State private var showingAnnouncements = false
    @State private var showingHackathon = false
    @State private var hackathonSection: String?
    @State private var showingTiers = false
    @State private var home: LivingSpaceProjection?
    @State private var failure: String?
    @State private var coverImage: PlatformImage?

    private var files: [ItemDetail] { model.core.items().filter { (home?.fileItems ?? []).contains($0.id) } }
    private var events: [ItemDetail] { model.core.items().filter { (home?.eventItems ?? []).contains($0.id) } }

    private var announcements: [ItemDetail] {
        let ids = home?.announcementItems ?? []
        return model.core.items().filter { ids.contains($0.id) }
    }

    private var space: SpaceSummary? { model.space(spaceId) }

    var body: some View {
        GlassEffectContainer(spacing: 24) {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                VStack(alignment: .leading, spacing: 12) {
                    if let coverImage {
                        #if canImport(UIKit)
                        Image(uiImage: coverImage).resizable().scaledToFill()
                            .frame(height: 160).clipped().clipShape(.rect(cornerRadius: 16))
                            .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder((colorScheme == .dark ? Color.white : Color.black).opacity(0.1), lineWidth: 1))
                        #else
                        Image(nsImage: coverImage).resizable().scaledToFill()
                            .frame(height: 160).clipped().clipShape(.rect(cornerRadius: 16))
                            .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder((colorScheme == .dark ? Color.white : Color.black).opacity(0.1), lineWidth: 1))
                        #endif
                    } else {
                        Image(systemName: "photo").font(.system(size: 32))
                            .frame(maxWidth: .infinity).frame(height: 160)
                            .background(.quaternary, in: .rect(cornerRadius: 16))
                            .accessibilityLabel("No cover image")
                    }
                    Text(space?.title ?? String(localized: "Space")).font(.largeTitle.bold())
                    Text("\(space?.members.filter { $0.kind == .person }.count ?? 0) members")
                        .font(.subheadline).foregroundStyle(.secondary)
                    if home?.canManage == true {
                        Menu("Set cover", systemImage: "photo.badge.plus") {
                            ForEach(files.filter { $0.file?.mime.hasPrefix("image/") == true }, id: \.id) { item in
                                Button(item.title) { command(["action": "set_cover", "file_id": item.id]) }
                            }
                        }.disabled(files.filter { $0.file?.mime.hasPrefix("image/") == true }.isEmpty)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading).padding(24)
                .glassEffect(.regular, in: RoundedRectangle(cornerRadius: 40))
                .accessibilityIdentifier("living-space.cover")

                section("Pinned announcements", symbol: "pin.fill") {
                    Button("View announcements", systemImage: "chevron.right") { showingAnnouncements = true }
                        .accessibilityIdentifier("living-space.announcements.open")
                    if announcements.isEmpty {
                        Text("No pinned announcements.").foregroundStyle(.secondary)
                    }
                    ForEach(announcements, id: \.id) { item in
                        Button(item.title) { model.push(.item(item.id)) }
                            .frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, 8)
                    }
                    if home?.canManage == true {
                        Menu("Pin announcement", systemImage: "pin") {
                            ForEach(pages, id: \.id) { item in
                                Button(item.title.isEmpty ? String(localized: "Untitled page") : item.title) {
                                    command(["action": "announce", "item_id": item.id])
                                }
                            }
                        }.disabled(pages.isEmpty)
                    }
                }

                section("Live page", symbol: "doc.text") {
                    ForEach(pages.filter { $0.id == home?.page }, id: \.id) { page in
                        Button { pageId = page.id } label: {
                            Label(page.title.isEmpty ? String(localized: "Untitled page") : page.title, systemImage: "doc.text")
                                .frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, 8)
                        }.accessibilityIdentifier("living-space.page.open")
                    }
                    if home?.page == nil {
                        Text("A shared page, edited together.").foregroundStyle(.secondary)
                    }
                    if home?.canManage == true {
                    Menu("Choose page", systemImage: "doc") {
                        ForEach(pages, id: \.id) { item in
                            Button(item.title.isEmpty ? String(localized: "Untitled page") : item.title) {
                                command(["action": "set_home", "page_id": item.id])
                            }
                        }
                    }.disabled(pages.isEmpty)
                    Button("Create page", systemImage: "plus") {
                        if let item = model.perform({ try model.core.pageCreate(spaceId: spaceId, title: "") }) {
                            model.refresh()
                            command(["action": "set_home", "page_id": item.id])
                            pageId = item.id
                        }
                    }
                    .buttonStyle(.glass)
                    }
                }

                section("Events", symbol: "calendar") {
                    if events.isEmpty { Text("No events yet.").foregroundStyle(.secondary) }
                    ForEach(events, id: \.id) { item in
                        NavigationLink(item.title) { LivingEventView(itemId: item.id) }
                            .frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, 8)
                    }
                }
                section("Files", symbol: "folder") {
                    if files.isEmpty { Text("No files yet.").foregroundStyle(.secondary) }
                    ForEach(files, id: \.id) { item in
                        Button { model.push(.item(item.id)) } label: {
                            HStack { FileThumb(item: item); Text(item.title); Spacer() }
                        }.padding(.vertical, 4)
                    }
                    Button("Open folder", systemImage: "folder") { model.push(.folder(spaceId)) }
                }
                VStack(spacing: 0) {
                    frozen("Mini-apps", symbol: "square.grid.2x2")
                }
                .padding(16).background(.thinMaterial, in: RoundedRectangle(cornerRadius: 20))

                section("Hackathon", symbol: "trophy") {
                    NavigationLink(home?.hackathon?.title ?? String(localized: "Hackathon")) {
                        LivingHackathonView(spaceId: spaceId)
                    }.accessibilityIdentifier("living-space.hackathon.open")
                }
                section("Memberships", symbol: "person.crop.circle.badge.checkmark") {
                    let mine = home?.tiers?.filter { $0.members.contains(model.me?.id ?? "") } ?? []
                    ForEach(mine) { tier in
                        Text(tier.name).font(.caption.weight(.medium)).padding(.horizontal, 8).padding(.vertical, 4)
                            .background(.quaternary, in: Capsule())
                    }
                    NavigationLink(home?.canManage == true ? String(localized: "Manage memberships") : String(localized: "Your memberships")) {
                        LivingTiersView(spaceId: spaceId)
                    }.accessibilityIdentifier("living-space.tiers.open")
                }
                section("Conversations", symbol: "bubble.left.and.bubble.right") {
                    if let conversation = home?.conversationSpace {
                        NavigationLink { LivingSpaceConversation(spaceId: conversation) } label: {
                            Label("Community conversation", systemImage: "lock.bubble")
                        }
                    } else {
                        Text(SyncModel.mode == .demo ? String(localized: "Encrypted conversation is not available on this demo account.") : String(localized: "Your encrypted conversation is being prepared.")).foregroundStyle(.secondary)
                    }
                }
                Text("Agents are paused in this Space.").font(.caption).foregroundStyle(.secondary)
                if let failure { Text(failure).font(.caption).foregroundStyle(.red).accessibilityIdentifier("living-space.error") }
            }
            .frame(maxWidth: 720).padding(20).frame(maxWidth: .infinity)
        }
        }
        .navigationTitle("Home")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .sheet(isPresented: $showingAnnouncements) {
            NavigationStack {
                List {
                    if announcements.isEmpty { Text("No pinned announcements.").foregroundStyle(.secondary) }
                    ForEach(announcements, id: \.id) { item in
                        Button(item.title) {
                            showingAnnouncements = false
                            model.push(.item(item.id))
                        }
                    }
                }
                .navigationTitle("Announcements")
                .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { showingAnnouncements = false } } }
            }
        }
        .navigationDestination(isPresented: Binding(get: { pageId != nil }, set: { if !$0 { pageId = nil } })) {
            if let pageId { PageScreen(itemId: pageId) }
        }
        .navigationDestination(isPresented: $showingHackathon) { LivingHackathonView(spaceId: spaceId, initialSection: hackathonSection) }
        .navigationDestination(isPresented: $showingTiers) { LivingTiersView(spaceId: spaceId) }
        .task(id: model.revision) {
            reloadHome()
        }
        .task(id: home?.coverFile) {
            guard let id = home?.coverFile,
                  let bytes = try? model.core.fileBytes(itemId: id, version: nil) else { coverImage = nil; return }
            coverImage = PlatformImage(data: bytes)
        }
        .accessibilityIdentifier("living-space.home")
        #if DEBUG
        .onReceive(NotificationCenter.default.publisher(for: Notification.Name("livingCaptureRoute"))) { notification in
            openCaptureScreen(notification.object as? String)
        }
        .task {
            // Explicit demo-only fixture uses real page and announcement mutations.
            if (SyncModel.mode == .demo && UserDefaults.standard.bool(forKey: "RodaLivingSpaceFixture")) || UserDefaults.standard.bool(forKey: "RodaLivingAccountFixture") {
                if let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first {
                    try? AppModel.sampleImage().write(to: documents.appendingPathComponent("living-space-cover.png"))
                }
                reloadHome()
                if home?.page == nil, home?.canManage == true,
                   let page = try? model.core.pageImportMarkdown(spaceId: spaceId, path: "welcome.md", markdown: "# Welcome to our Space\n\nA shared place to plan, learn and stay connected.\n\n## This week\n\n- [ ] Share your ideas\n- [ ] Plan our next gathering\n\nEveryone can help keep this page up to date.") {
                    command(["action": "set_home", "page_id": page.id])
                    command(["action": "announce", "item_id": page.id])
                    if let cover = try? model.core.fileAdd(spaceId: spaceId, path: "cover.png", name: "Space cover", mime: "image/png", bytes: AppModel.sampleImage(), thumbnail: AppModel.sampleImage(side: 120)) {
                        command(["action": "set_cover", "file_id": cover.id])
                    }
                }
            }
            if UserDefaults.standard.bool(forKey: "RodaLivingAccountFixture") && UserDefaults.standard.bool(forKey: "RodaLivingExpandedFixture") {
                seedExpandedFixture()
            }
            if SyncModel.mode == .demo && UserDefaults.standard.bool(forKey: "RodaLivingJoinFixture") && (home?.pendingRequests ?? []).isEmpty {
                command(["action": "demo_join_request"])
            }
            // Opens real components for capture; it never fabricates approvals.
            openCaptureScreen(UserDefaults.standard.string(forKey: "RodaLivingSpaceScreen"))
        }
        #endif
    }

    #if DEBUG
    private func seedExpandedFixture() {
        reloadHome()
        if home?.hackathon == nil {
            let deadline = Int64(Date().addingTimeInterval(100).timeIntervalSince1970 * 1000)
            command(["action": "hackathon_create", "title": "Build together", "deadline_ms": deadline])
            command(["action": "register"])
            command(["action": "team_create", "name": "Garden builders"])
            command(["action": "schedule_event", "title": "Kickoff", "at_ms": deadline - 3600000])
            if let me = model.me?.id { command(["action": "set_judge", "identity_id": me]) }
            if let team = home?.hackathon?.teams.first {
                command(["action": "project_submit", "team_id": team.id, "url": "https://example.com/garden", "file_ids": home?.fileItems ?? []])
            }
        }
        if (home?.tiers ?? []).isEmpty {
            command(["action": "tier_create", "name": "Builder"])
            if let tier = home?.tiers?.first {
                if let me = model.me?.id { command(["action": "tier_assign", "tier_id": tier.id, "identity_id": me]) }
                command(["action": "tier_group_create", "tier_id": tier.id, "title": "Builder lounge"])
                if let partition = tier.space, let page = try? model.core.pageImportMarkdown(spaceId: partition, path: "builder-notes.md", markdown: "# Builder notes\n\nA shared page for community builders.") {
                    command(["action": "tier_bind", "tier_id": tier.id, "item_id": page.id, "kind": "page"])
                }
            }
        }
    }

    private func openCaptureScreen(_ screen: String?) {
        showingAnnouncements = false
        showingHackathon = false
        showingTiers = false
        pageId = nil
        switch screen {
        case "announcements": showingAnnouncements = true
        case "page": reloadHome(); pageId = home?.page
        case "hackathon", "teams", "submission", "results":
            hackathonSection = screen == "hackathon" ? nil : screen
            showingHackathon = true
        case "tiers": showingTiers = true
        default: break
        }
    }
    #endif

    private func reloadHome() {
        do {
            try model.core.setLivingSpaceEnabled(enabled: true)
            let json = try model.core.livingSpaceCommand(spaceId: spaceId, commandJson: "{\"action\":\"home\"}")
            home = try LivingSpaceProjection.decode(json)
            pages = model.core.items().filter { $0.spaceId == spaceId && $0.kindId == "page" }
            failure = nil
        } catch {
            failure = error.localizedDescription
        }
    }

    private func command(_ value: [String: Any]) {
        do {
            let json = String(decoding: try JSONSerialization.data(withJSONObject: value), as: UTF8.self)
            _ = try model.core.livingSpaceCommand(spaceId: spaceId, commandJson: json)
            model.refresh()
            reloadHome()
        } catch { failure = error.localizedDescription }
    }

    private func section<Content: View>(_ title: LocalizedStringKey, symbol: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Label(title, systemImage: symbol).font(.headline)
            content()
        }.frame(maxWidth: .infinity, alignment: .leading)
    }

    private func frozen(_ title: LocalizedStringKey, symbol: String) -> some View {
        HStack(spacing: 12) {
            Image(systemName: symbol).frame(width: 24).accessibilityHidden(true)
            Text(title)
            Spacer()
            Text("Coming later").font(.caption).foregroundStyle(.secondary)
            Image(systemName: "lock").font(.caption).foregroundStyle(.secondary)
        }
        .padding(.vertical, 12)
        .accessibilityElement(children: .combine)
    }
}

/// Human-only conversation: deliberately does not invoke AppModel's agent planner.
struct LivingSpaceConversation: View {
    @Environment(AppModel.self) private var model
    let spaceId: String
    @State private var entries: [TimelineEntry] = []
    @State private var draft = ""

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 16) {
                ForEach(entries, id: \.id) { entry in
                    if case .message(let text, _) = entry.kind {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(entry.author.name).font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                            Text(text).textSelection(.enabled)
                        }.frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                if entries.isEmpty { Text("Start the conversation.").foregroundStyle(.secondary) }
            }.padding(20).frame(maxWidth: 720).frame(maxWidth: .infinity)
        }
        .navigationTitle("General conversation")
        .safeAreaInset(edge: .bottom) {
            HStack {
                TextField("Message", text: $draft, axis: .vertical).lineLimit(1...6)
                    .accessibilityIdentifier("living-space.message")
                Button("Send", systemImage: "arrow.up.circle.fill") {
                    let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
                    guard !text.isEmpty else { return }
                    if model.perform({ try model.core.sendMessage(spaceId: spaceId, text: text) }) != nil {
                        draft = ""
                        model.refresh()
                    }
                }
                .labelStyle(.iconOnly).disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .accessibilityIdentifier("living-space.send")
            }
            .padding(16).background(.bar)
        }
        .task(id: model.revision) {
            entries = ((try? model.core.timeline(spaceId: spaceId)) ?? []).filter { $0.author.kind == .person && $0.inThread == nil }
        }
    }
}
