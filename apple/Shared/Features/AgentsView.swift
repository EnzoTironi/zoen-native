import SwiftUI
import RodaCore

/// "Seus agentes" (poster #026): seus agentes e, em Explorar, os de outras pessoas
/// que convivem com você. Cada cartão mostra dono, função e onde está disponível.
struct AgentsLibraryScreen: View {
    @Environment(AppModel.self) private var model
    @State private var segment = 0

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("Agents are members: they have an owner, a role, per-chat permissions and a budget. Whoever owns the agent pays for it.")
                    .font(.subheadline).foregroundStyle(Palette.textSecondary)
                    .padding(.horizontal, 4)
                Picker("", selection: $segment) {
                    Text("My agents").tag(0)
                    Text("Other people’s").tag(1)
                }
                .pickerStyle(.segmented)

                let list = model.agents.filter { segment == 0 ? $0.persona.isMine : !$0.persona.isMine }
                ForEach(list) { a in
                    NavigationLink(value: Route.agent(a.id)) { AgentLibraryCard(profile: a) }
                        .buttonStyle(.plain)
                }
                if segment == 0 {
                    PhaseNote(symbol: "plus.app", title: String(localized: "Creating agents arrives in phase 2"),
                              text: String(localized: "Knowledge, tools, versions and publishing (posters #028–#035). Today the agents come from the demo."))
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
    }
}

struct AgentLibraryCard: View {
    let profile: AgentProfile
    var body: some View {
        HStack(alignment: .center, spacing: 14) {
            AgentAvatar(persona: profile.persona, size: 52, state: profile.pendingRequests > 0 ? .waiting : .idle)
            VStack(alignment: .leading, spacing: 3) {
                Text(profile.persona.name).font(.headline).foregroundStyle(Palette.textPrimary)
                Text(profile.persona.bio).font(.subheadline).foregroundStyle(Palette.textSecondary).lineLimit(2)
                HStack(spacing: 6) {
                    Text(profile.persona.isMine ? String(localized: "You") : String(localized: "\(profile.persona.ownerName ?? "")’s"))
                        .foregroundStyle(Palette.textSecondary)
                    Text("·").foregroundStyle(Palette.textTertiary)
                    Text(profile.spaces.count == 1 ? String(localized: "In 1 chat") : String(localized: "In \(profile.spaces.count) chats"))
                        .foregroundStyle(Palette.action)
                }
                .font(.caption.weight(.medium))
            }
            Spacer(minLength: 6)
            if let s = profile.budgetSpentCents, let l = profile.budgetLimitCents {
                BudgetRing(spent: s, limit: l, size: 30, lineWidth: 4)
            }
            Image(systemName: "chevron.right").font(.caption.weight(.bold)).foregroundStyle(Palette.textTertiary)
        }
        .solidCard(radius: 20, padding: 14)
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
                            Text("\(a.spaceTitle) · \(RodaTime.relative(a.atMs))").font(.caption).foregroundStyle(Palette.textSecondary)
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
