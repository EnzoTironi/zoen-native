import SwiftUI
import ClientCore

/// "Seus agentes": the agents you own and the ones you talk to, as citizens. Each row
/// shows who it is, whether it's on (yours only: the relay tells owners) and how many
/// of its notes are waiting for you.
struct AgentsLibraryScreen: View {
    @Environment(AppModel.self) private var model

    private var mine: [AgentProfile] { model.agents.filter(\.persona.isMine) }
    private var others: [AgentProfile] { model.agents.filter { !$0.persona.isMine } }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                section(String(localized: "Agents you own"), mine, id: "agents.mine") {
                    PhaseNote(symbol: "sparkles", title: String(localized: "No agents yet"),
                              text: String(localized: "When you create one, it shows up here with its link to share."))
                }
                if !others.isEmpty {
                    section(String(localized: "In your chats"), others, id: "agents.others") { EmptyView() }
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .frame(maxWidth: 720)
            .frame(maxWidth: .infinity)
        }
        .scrollEdgeEffectStyle(.soft, for: .top)
        .background(NightBackdrop())
        .navigationTitle("Your agents")
        .task(id: mine.map(\.id)) { await model.agentStatuses.load(mine.map(\.id), core: model.core) }
    }

    private func section<Empty: View>(_ title: String, _ list: [AgentProfile], id: String,
                                      @ViewBuilder empty: () -> Empty) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionHeader(title: title)
            if list.isEmpty { empty() }
            ForEach(list) { a in
                NavigationLink(value: Route.agent(a.id)) { AgentLibraryCard(profile: a) }
                    .buttonStyle(PressScaleStyle())
                    .accessibilityIdentifier("agentRow.\(a.persona.handle)")
            }
        }
        .accessibilityIdentifier(id)
    }
}

struct AgentLibraryCard: View {
    @Environment(AppModel.self) private var model
    let profile: AgentProfile
    private var status: AgentStatus? { model.agentStatuses.known[profile.id] }

    var body: some View {
        HStack(alignment: .center, spacing: 14) {
            AgentAvatar(persona: profile.persona, size: 52, state: profile.ownerNotesPending > 0 ? .waiting : .idle)
                .saturation(status == .revoked ? 0 : 1)
            VStack(alignment: .leading, spacing: 5) {
                Text(profile.persona.name).font(.headline).foregroundStyle(Palette.textPrimary)
                Text(subtitle).font(.subheadline).foregroundStyle(Palette.textSecondary).lineLimit(1)
                if status != nil || profile.ownerNotesPending > 0 {
                    HStack(spacing: 6) {
                        if let status { AgentStatusPill(status: status) }
                        if profile.ownerNotesPending > 0 { AgentPendingBadge(count: profile.ownerNotesPending) }
                    }
                    .animation(.easeOut(duration: 0.2), value: status)
                }
            }
            Spacer(minLength: 6)
            Image(systemName: "chevron.right").font(.caption.weight(.bold)).foregroundStyle(Palette.textTertiary)
        }
        .glassCard(radius: 20, padding: 14)
        .contentShape(.rect)
    }

    private var subtitle: String {
        let chats = profile.spaces.count == 1 ? String(localized: "In 1 chat") : String(localized: "In \(profile.spaces.count) chats")
        if profile.persona.isMine { return chats }
        return String(localized: "\(profile.persona.ownerName ?? String(localized: "Someone"))’s · \(chats)")
    }
}

extension DecisionKind {
    var uiSymbol: String {
        switch self {
        case .act: "checkmark.circle.fill"
        case .actWithUndo: "arrow.uturn.backward.circle.fill"
        case .request: "hand.raised.circle.fill"
        case .block: "xmark.circle.fill"
        }
    }
    var uiColor: Color {
        switch self {
        case .act: Palette.success
        case .actWithUndo: Palette.action
        case .request: Palette.amber
        case .block: Palette.textTertiary
        }
    }
    var uiLabel: String {
        switch self {
        case .act: String(localized: "Does it")
        case .actWithUndo: String(localized: "Does it · Undo")
        case .request: String(localized: "Asks")
        case .block: String(localized: "Can’t")
        }
    }
}

/// "Financeiro nesta conversa" (poster #038). A autonomia é real (Concessão assinada
/// no log); "quem pode acionar" e "histórico" mostram a regra fixa deste protótipo;
/// "ferramentas" é a tabela de decisões avaliada pelo núcleo para este nível.
struct AgentPermissionsView: View {
    @Environment(AppModel.self) private var model
    let agentId: String
    let spaceId: String
    @State private var previews: [DecisionPreview] = []

    private var profile: AgentProfile? { model.agentProfile(agentId) }
    private var level: TrustLevelDto? { profile?.spaces.first { $0.spaceId == spaceId }?.level }

    var body: some View {
        List {
            if let p = profile {
                Section {
                    HStack(spacing: 14) {
                        AgentAvatar(persona: p.persona, size: 52)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(p.persona.name).font(.title3.weight(.bold))
                            Text(p.persona.isMine ? String(localized: "Your agent") : String(localized: "\(p.persona.ownerName ?? "")’s agent"))
                                .font(.subheadline).foregroundStyle(Palette.textSecondary)
                        }
                    }
                    Text(p.persona.bio).font(.subheadline).foregroundStyle(Palette.textSecondary)
                }

                Section("Who can trigger it") {
                    Label("Members of this chat", systemImage: "person.2")
                    Text("Any member can mention @\(p.persona.name). Only \(p.persona.isMine ? String(localized: "you") : (p.persona.ownerName ?? String(localized: "the owner"))) can change its autonomy.")
                        .font(.caption).foregroundStyle(Palette.textSecondary)
                }

                Section("History") {
                    Label("Since it joined the chat", systemImage: "clock.arrow.circlepath")
                    Text("The agent is a declared member: it reads what arrives after it joined, like anyone else. That’s in the log.")
                        .font(.caption).foregroundStyle(Palette.textSecondary)
                }

                Section {
                    ForEach(previews) { d in
                        HStack(alignment: .top, spacing: 10) {
                            Image(systemName: d.kind.uiSymbol).foregroundStyle(d.kind.uiColor).frame(width: 22)
                            VStack(alignment: .leading, spacing: 1) {
                                Text(d.action).font(.subheadline.weight(.medium))
                                Text(d.example).font(.caption).foregroundStyle(Palette.textSecondary)
                            }
                            Spacer()
                            Text(d.kind.uiLabel).font(.caption.weight(.semibold)).foregroundStyle(d.kind.uiColor)
                        }
                    }
                } header: {
                    Text("Tools")
                } footer: {
                    Text("Computed by the core for the chosen level. Red lines (money above \(Money.format(10_000)), a public audience, third-party data) always ask.")
                }

                Section {
                    let decisions = model.standing(for: agentId, space: spaceId)
                    if decisions.isEmpty {
                        Text("None yet.").font(.subheadline).foregroundStyle(Palette.textSecondary)
                    }
                    ForEach(decisions, id: \.grantId) { d in
                        StandingDecisionRow(decision: d, showSpace: false)
                    }
                } header: {
                    Text("Standing decisions")
                } footer: {
                    Text("Set by swiping up (always approve) or down (always deny) on an approval. Revoke one and the agent asks again.")
                }

                Section {
                    if let level {
                        Picker("Autonomy", selection: Binding(get: { level }, set: { new in
                            withAnimation(.snappy) {
                                _ = model.perform { try model.core.setTrust(agentId: agentId, spaceId: spaceId, level: new) }
                            }
                            loadPreviews()
                        })) {
                            ForEach(TrustLevelDto.allCases, id: \.self) { l in Text(l.label).tag(l) }
                        }
                        .pickerStyle(.segmented)
                        .disabled(!p.persona.isMine)
                        Text(level.explanation).font(.subheadline).foregroundStyle(Palette.textSecondary)
                    }
                } header: {
                    Text("Autonomy in this chat")
                } footer: {
                    Text(p.persona.isMine ? String(localized: "Changes right away: it becomes a signed Grant in this Space’s log.") : String(localized: "Only \(p.persona.ownerName ?? String(localized: "the owner")) can change this agent’s autonomy."))
                }
            }
        }
        .navigationTitle("\(profile?.persona.name ?? String(localized: "Agent")) in this chat")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .onAppear(perform: loadPreviews)
    }

    private func loadPreviews() {
        previews = model.core.previewDecisions(agentId: agentId, spaceId: spaceId)
    }
}

/// "Atividades recentes" do agente (poster #030), lidas do log assinado.
struct AgentActivitySection: View {
    @Environment(AppModel.self) private var model
    let agentId: String
    @State private var acts: [AgentActivityDto] = []

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionHeader(title: String(localized: "Recent activity"), trailing: String(localized: "from the signed log"))
            VStack(spacing: 0) {
                ForEach(Array(acts.prefix(6).enumerated()), id: \.offset) { i, a in
                    HStack(alignment: .top, spacing: 12) {
                        Image(systemName: symbol(a.label)).foregroundStyle(Palette.success).frame(width: 22)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("\(a.label): \(a.detail)").font(.subheadline).lineLimit(2)
                            Text("\(a.spaceTitle) · \(MessageTime.relative(a.atMs))").font(.caption).foregroundStyle(Palette.textSecondary)
                        }
                        Spacer(minLength: 8)
                        if let c = a.costCents {
                            Text(Money.format(c)).font(.subheadline.weight(.semibold)).monospacedDigit()
                        }
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 11)
                    if i < min(acts.count, 6) - 1 { Divider().padding(.leading, 48).opacity(0.5) }
                }
                if acts.isEmpty {
                    InkEmptyState(pose: .zen, title: String(localized: "Nothing yet."), size: 84)
                }
            }
            .background(Palette.surface, in: .rect(cornerRadius: 20, style: .continuous))
        }
        .task(id: model.revision) { acts = (try? model.core.agentActivity(agentId: agentId)) ?? [] }
    }

    private func symbol(_ label: String) -> String {
        switch label {
        case String(localized: "Asked for approval"): "hand.raised.fill"
        case String(localized: "Created"), String(localized: "Delivered an Item"): "doc.badge.plus"
        case String(localized: "Edited"): "pencil"
        default: "checkmark.circle.fill"
        }
    }
}
