import SwiftUI
import RodaCore

/// Aba 4: você, seus agentes, seus Itens, segurança e uma única tela de configurações.
struct YouScreen: View {
    @Environment(AppModel.self) private var model
    @State private var confirmReset = false
    @AppStorage("RodaHeaderMenuTrial") private var headerMenuTrial = false

    var body: some View {
        List {
            if let me = model.me {
                Section {
                    HStack(spacing: 14) {
                        PersonAvatar(persona: me, size: 56)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(me.name).font(.title3.weight(.bold))
                            Text("Your personal space · @\(me.handle)").font(.subheadline).foregroundStyle(Palette.textSecondary)
                            Text("Identity \(String(me.id.prefix(8)))…\(String(me.id.suffix(4)))")
                                .font(.caption.monospaced()).foregroundStyle(Palette.textTertiary)
                        }
                    }
                    .padding(.vertical, 4)
                    Text("Organize what’s yours and decide who can use it.")
                        .font(.subheadline).foregroundStyle(Palette.textSecondary)
                    HStack(spacing: 12) {
                        Image(systemName: "lock.fill").foregroundStyle(Palette.action)
                            .frame(width: 36, height: 36)
                            .background(Palette.action.opacity(0.12), in: .rect(cornerRadius: 10, style: .continuous))
                        VStack(alignment: .leading, spacing: 1) {
                            Text("Only you and authorized agents").font(.subheadline.weight(.semibold))
                            Text(SyncModel.mode == .demo ? String(localized: "Everything stays on this device, signed with your key.") : String(localized: "Your space stays on this device. Chats sync, signed with your key.")).font(.caption).foregroundStyle(Palette.textSecondary)
                        }
                    }
                }
            }

            AccountSection()

            Section("Your data") {
                NavigationLink(value: Route.items) {
                    LabeledContent { Text("\(model.stats?.items ?? 0)") } label: { Label("Items", systemImage: "square.stack.3d.up") }
                }
                Button { model.select(.conversations) } label: {
                    LabeledContent { Text("\(model.spaces.count)") } label: {
                        Label("Chats", systemImage: "bubble.left.and.bubble.right").foregroundStyle(Palette.textPrimary)
                    }
                }
                if model.stats?.allLogsValid == false {
                    NavigationLink(value: Route.integrity) {
                        Label {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("Some chats look damaged")
                                Text("Open to see which ones need attention.")
                                    .font(.caption).foregroundStyle(Palette.textSecondary)
                            }
                        } icon: { Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(Palette.amber) }
                    }
                }
            }

            Section {
                ForEach(model.agents.filter { $0.persona.isMine }) { a in
                    NavigationLink(value: Route.agent(a.id)) { AgentRow(profile: a) }
                }
            } header: {
                Text("Access grants")
            } footer: {
                Text("Each of your agents has a trust level per chat and a monthly budget.")
            }

            MiniAppAccessSection()

            Section {
                HStack(spacing: 14) {
                    Image(systemName: "person.badge.key.fill").font(.title2).foregroundStyle(Palette.action)
                        .frame(width: 44, height: 44).background(Palette.action.opacity(0.12), in: .rect(cornerRadius: 12, style: .continuous))
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Protect what’s yours").font(.headline)
                        Text("A passkey with Face ID keeps your identity on all your devices.")
                            .font(.caption).foregroundStyle(Palette.textSecondary)
                    }
                }
                .padding(.vertical, 4)
                Button("Soon (weeks 5–6)") {}.disabled(true)
            } footer: {
                Text("Your identity will unlock with Face ID on every device you own.")
            }

            #if DEBUG && os(iOS)
            Section {
                Toggle("Header menu", isOn: $headerMenuTrial)
            } header: {
                Text("Trials")
            } footer: {
                Text("Tap a chat’s name to open its menu. Not shipped.")
            }
            #endif

            Section {
                LabeledContent("Agent") { Text(model.planner.availabilityLabel).multilineTextAlignment(.trailing) }
                if SyncModel.mode == .demo {
                    Button("Restart the demo", role: .destructive) { confirmReset = true }
                }
            } header: {
                Text("Settings")
            } footer: {
                if SyncModel.mode == .demo {
                    Text("Demo · illustrative data. Agent payments and messages are simulated.")
                } else {
                    Text("Your chats stay in sync across your devices.")
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(NightBackdrop())
        .navigationTitle("Your context")
        .confirmationDialog("Erase everything and restart the demo story?", isPresented: $confirmReset, titleVisibility: .visible) {
            Button("Restart", role: .destructive) {
                model.perform { try model.core.resetDemo() }
            }
        }
    }
}

struct AgentRow: View {
    let profile: AgentProfile
    var body: some View {
        HStack(spacing: 12) {
            AgentAvatar(persona: profile.persona, size: 40, budgetFraction: fraction)
            VStack(alignment: .leading, spacing: 2) {
                Text(profile.persona.name).font(.body.weight(.semibold))
                Text(subtitle).font(.caption).foregroundStyle(Palette.textSecondary).lineLimit(1)
            }
            Spacer()
            if let s = profile.budgetSpentCents, let l = profile.budgetLimitCents {
                BudgetRing(spent: s, limit: l, size: 26, lineWidth: 4)
            }
        }
        .padding(.vertical, 2)
    }

    private var fraction: Double? {
        guard let s = profile.budgetSpentCents, let l = profile.budgetLimitCents, l > 0 else { return nil }
        return Double(s) / Double(l)
    }

    private var subtitle: String {
        if !profile.persona.isMine { return String(localized: "\(profile.persona.ownerName ?? String(localized: "someone else"))’s · \(profile.spaces.count) Space(s) with you") }
        let levels = Set(profile.spaces.map(\.level.label)).sorted().joined(separator: ", ")
        return levels.isEmpty ? String(localized: "No Spaces") : String(localized: "\(levels) · \(profile.spaces.count) Space(s)")
    }
}

/// Perfil do agente: orçamento, confiança por Espaço e o que isso muda (avaliado no núcleo).
struct AgentDetailView: View {
    @Environment(AppModel.self) private var model
    let agentId: String
    @State private var selectedSpace: String?
    @State private var previews: [DecisionPreview] = []

    private var profile: AgentProfile? { model.agentProfile(agentId) }

    var body: some View {
        ScrollView {
            if let p = profile {
                VStack(alignment: .leading, spacing: 22) {
                    VStack(spacing: 10) {
                        AgentAvatar(persona: p.persona, size: 96)
                        Text(p.persona.name).font(.title.weight(.bold))
                        Text(p.persona.isMine ? String(localized: "Your agent") : String(localized: "\(p.persona.ownerName ?? "")’s agent"))
                            .font(.subheadline.weight(.medium)).foregroundStyle(Color(hex: p.persona.tintHex))
                        Text(p.persona.bio).font(.subheadline).foregroundStyle(Palette.textSecondary).multilineTextAlignment(.center)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.top, 8)

                    budget(p)
                    AgentActivitySection(agentId: agentId)
                    trust(p)
                    if !previews.isEmpty { decisionTable }
                    permissionsLinks(p)
                }
                .padding(.horizontal, 18)
                .padding(.bottom, 30)
                .frame(maxWidth: 680)
                .frame(maxWidth: .infinity)
            }
        }
        .background(NightBackdrop())
        .navigationTitle(profile?.persona.name ?? String(localized: "Agent"))
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .onAppear {
            if selectedSpace == nil { selectedSpace = profile?.spaces.first(where: { $0.spaceTitle.contains("Paraty") })?.spaceId ?? profile?.spaces.first?.spaceId }
            loadPreviews()
        }
        .onChange(of: selectedSpace) { _, _ in loadPreviews() }
        .onChange(of: model.revision) { _, _ in loadPreviews() }
    }

    private func budget(_ p: AgentProfile) -> some View {
        Group {
            if let spent = p.budgetSpentCents, let limit = p.budgetLimitCents {
                HStack(spacing: 16) {
                    BudgetRing(spent: spent, limit: limit, size: 72, lineWidth: 8, showsLabel: true)
                    VStack(alignment: .leading, spacing: 4) {
                        Text("\(Money.format(spent)) of \(Money.format(limit))").font(.title3.weight(.semibold)).monospacedDigit()
                            .lineLimit(1).minimumScaleFactor(0.7)
                        Text(p.nearLimit ? String(localized: "Near the limit. At the cap, it stops.") : String(localized: "This month’s AI budget"))
                            .font(.subheadline).foregroundStyle(p.nearLimit ? Palette.amber : Palette.textSecondary)
                    }
                    Spacer()
                    Button("+" + Money.format(2_000)) { model.perform { try model.core.raiseBudget(agentId: p.id, extraCents: 2_000) } }
                        .buttonStyle(.glass)
                }
                .solidCard()
            } else {
                Label("The budget belongs to \(p.persona.ownerName ?? String(localized: "someone else")). Whoever owns the agent pays for it.", systemImage: "info.circle")
                    .font(.subheadline).foregroundStyle(Palette.textSecondary)
                    .solidCard()
            }
        }
    }

    private func permissionsLinks(_ p: AgentProfile) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionHeader(title: String(localized: "Permissions per chat"))
            VStack(spacing: 0) {
                ForEach(p.spaces) { s in
                    NavigationLink(value: Route.permissions(agent: p.id, space: s.spaceId)) {
                        HStack {
                            Image(systemName: s.level.symbol).foregroundStyle(Palette.action).frame(width: 24)
                            Text(s.spaceTitle).font(.subheadline.weight(.medium)).foregroundStyle(Palette.textPrimary)
                            Spacer()
                            Text(s.level.label).font(.subheadline).foregroundStyle(Palette.textSecondary)
                            Image(systemName: "chevron.right").font(.caption.weight(.bold)).foregroundStyle(Palette.textTertiary)
                        }
                        .padding(.horizontal, 14).padding(.vertical, 12)
                        .contentShape(.rect)
                    }
                    .buttonStyle(.plain)
                    if s.id != p.spaces.last?.id { Divider().padding(.leading, 52).opacity(0.5) }
                }
            }
            .background(Palette.surface, in: .rect(cornerRadius: 20, style: .continuous))
        }
    }

    private func trust(_ p: AgentProfile) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionHeader(title: String(localized: "Trust per Space"))
            VStack(alignment: .leading, spacing: 14) {
                if p.spaces.count > 1 {
                    Picker("Space", selection: $selectedSpace) {
                        ForEach(p.spaces) { s in Text(s.spaceTitle).tag(Optional(s.spaceId)) }
                    }
                    .pickerStyle(.menu)
                }
                if let s = p.spaces.first(where: { $0.spaceId == selectedSpace }) {
                    HStack {
                        Text(s.spaceTitle).font(.headline)
                        Spacer()
                    }
                    Picker("Level", selection: Binding(
                        get: { s.level },
                        set: { new in
                            withAnimation(.snappy) {
                                _ = model.perform { try model.core.setTrust(agentId: p.id, spaceId: s.spaceId, level: new) }
                            }
                        })) {
                        ForEach(TrustLevelDto.allCases, id: \.self) { l in Text(l.label).tag(l) }
                    }
                    .pickerStyle(.segmented)
                    .disabled(!p.persona.isMine)
                    Text(s.level.explanation).font(.subheadline).foregroundStyle(Palette.textSecondary)
                        .contentTransition(.opacity)
                    if !p.persona.isMine {
                        Label("Only \(p.persona.ownerName ?? String(localized: "the owner")) can change this agent’s trust.", systemImage: "lock")
                            .font(.caption).foregroundStyle(Palette.textTertiary)
                    }
                }
            }
            .solidCard()
        }
    }

    private var decisionTable: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionHeader(title: String(localized: "What this changes"), trailing: String(localized: "evaluated by the core"))
            VStack(spacing: 0) {
                ForEach(previews) { d in
                    HStack(alignment: .top, spacing: 12) {
                        Image(systemName: symbol(d.kind)).font(.system(size: 18, weight: .semibold)).foregroundStyle(color(d.kind)).frame(width: 26)
                        VStack(alignment: .leading, spacing: 2) {
                            HStack(spacing: 6) {
                                Text(d.action).font(.subheadline.weight(.semibold))
                                if d.redLine {
                                    Text("red line").font(.caption2.weight(.bold)).foregroundStyle(Palette.danger)
                                        .padding(.horizontal, 6).padding(.vertical, 1).background(Palette.danger.opacity(0.12), in: .capsule)
                                }
                            }
                            Text(d.example).font(.caption).foregroundStyle(Palette.textSecondary)
                        }
                        Spacer(minLength: 8)
                        Text(label(d.kind)).font(.caption.weight(.semibold)).foregroundStyle(color(d.kind))
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 11)
                    .contentTransition(.opacity)
                    if d.id != previews.last?.id { Divider().padding(.leading, 52).opacity(0.5) }
                }
            }
            .background(Palette.surface, in: .rect(cornerRadius: 20, style: .continuous))
            Text("Reversible → does it and shows Undo. Irreversible or external → asks. Red lines ask at any level.")
                .font(.caption).foregroundStyle(Palette.textTertiary)
        }
        .animation(.snappy, value: previews.map(\.kind))
    }

    private func loadPreviews() {
        guard let s = selectedSpace else { return }
        previews = model.core.previewDecisions(agentId: agentId, spaceId: s)
    }

    private func symbol(_ k: DecisionKind) -> String {
        switch k {
        case .act: "checkmark.circle.fill"
        case .actWithUndo: "arrow.uturn.backward.circle.fill"
        case .request: "hand.raised.circle.fill"
        case .block: "xmark.circle.fill"
        }
    }

    private func color(_ k: DecisionKind) -> Color {
        switch k {
        case .act: Palette.success
        case .actWithUndo: Palette.action
        case .request: Palette.amber
        case .block: Palette.textTertiary
        }
    }

    private func label(_ k: DecisionKind) -> String {
        switch k {
        case .act: String(localized: "Does it")
        case .actWithUndo: String(localized: "Does it · Undo")
        case .request: String(localized: "Asks")
        case .block: String(localized: "Can’t")
        }
    }
}

/// O log de eventos: invisível no fluxo normal, auditável aqui.
struct IntegrityView: View {
    @Environment(AppModel.self) private var model
    @State private var reports: [LogReport] = []

    var body: some View {
        List {
            Section {
                ForEach(reports) { r in
                    NavigationLink(value: Route.log(r.spaceId)) {
                        HStack(spacing: 12) {
                            Image(systemName: r.valid ? "checkmark.seal.fill" : "exclamationmark.octagon.fill")
                                .foregroundStyle(r.valid ? Palette.success : Palette.danger)
                                .font(.title3)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(r.spaceTitle).font(.body.weight(.medium))
                                Text(r.valid ? String(localized: "\(r.events) events · head \(String(r.headHash.prefix(10)))…") : (r.error ?? String(localized: "invalid")))
                                    .font(.caption.monospaced()).foregroundStyle(Palette.textSecondary)
                            }
                        }
                    }
                }
            } footer: {
                Text("Each Space is an append-only log. Each event carries the previous hash and is signed (Ed25519) by whoever created it. Verification rereads the disk and recomputes everything in the Rust core.")
            }
        }
        .navigationTitle("Chat history")
        .task(id: model.revision) { reports = model.core.verifyAll() }
    }
}

struct LogEventsView: View {
    @Environment(AppModel.self) private var model
    let spaceId: String
    @State private var events: [LogEventDto] = []

    var body: some View {
        List(events) { e in
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Text("#\(e.seq)").font(.caption.monospaced().weight(.bold)).foregroundStyle(Palette.action)
                    Text(e.label).font(.subheadline.weight(.medium)).lineLimit(1)
                    Spacer()
                    Text(RodaTime.relative(e.atMs)).font(.caption).foregroundStyle(Palette.textTertiary)
                }
                HStack(spacing: 6) {
                    Avatar(persona: e.author, size: 16)
                    Text(e.author.name).font(.caption).foregroundStyle(Palette.textSecondary)
                }
                Group {
                    Text("hash \(e.hash.prefix(24))…")
                    Text("prev \(e.prev.prefix(24))…")
                    Text("sig  \(e.signature.prefix(24))…")
                }
                .font(.caption2.monospaced())
                .foregroundStyle(Palette.textTertiary)
            }
            .padding(.vertical, 2)
        }
        .navigationTitle(model.space(spaceId)?.title ?? "Log")
        .task(id: model.revision) { events = (try? model.core.logEvents(spaceId: spaceId)) ?? [] }
    }
}

struct ItemsListView: View {
    @Environment(AppModel.self) private var model
    @State private var items: [ItemDetail] = []
    var onOpen: (String) -> Void

    var body: some View {
        List(items) { item in
            Button { onOpen(item.id) } label: {
                HStack(spacing: 12) {
                    Image(systemName: item.kindId == "plan" ? "map" : (item.kindId == "task" ? "checkmark.circle" : "doc.text"))
                        .foregroundStyle(Palette.action).frame(width: 28)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(item.title).font(.body.weight(.medium)).foregroundStyle(Palette.textPrimary)
                        Text("\(item.kindLabel) · \(item.spaceTitle) · v\(item.version)").font(.caption).foregroundStyle(Palette.textSecondary)
                    }
                    Spacer()
                    if let t = item.plan?.totalCents { Text(Money.format(t)).font(.subheadline).monospacedDigit().foregroundStyle(Palette.textSecondary) }
                }
            }
        }
        .navigationTitle("Items")
        .task(id: model.revision) { items = model.core.items() }
    }
}
