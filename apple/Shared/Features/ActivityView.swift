import SwiftUI
import RodaCore

/// Aba 2: o que precisa de você e o que os agentes fizeram por você.
/// É aqui que o modelo de confiança vive: pedidos em lote, orçamento e menções.
struct ActivityScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dynamicTypeSize) private var typeSize

    /// Em tamanhos de acessibilidade o cabeçalho empilha (texto em cima, botão embaixo)
    /// em vez de espremer o texto ao lado do botão.
    private var rowLayout: AnyLayout {
        typeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 10))
            : AnyLayout(HStackLayout(spacing: 12))
    }

    private var pendingByAgent: [(Persona, [AgentRequestDto])] {
        let pending = model.pendingRequests
        var order: [String] = []
        var groups: [String: (Persona, [AgentRequestDto])] = [:]
        for r in pending {
            if groups[r.agent.id] == nil { order.append(r.agent.id); groups[r.agent.id] = (r.agent, []) }
            groups[r.agent.id]!.1.append(r)
        }
        return order.compactMap { groups[$0] }
    }

    private var resolved: [AgentRequestDto] {
        model.requests.filter { $0.status == .approved || $0.status == .denied }
    }

    enum Filter: String, CaseIterable, Identifiable {
        case mentions, tasks, approvals
        var id: String { rawValue }
        var label: String {
            switch self {
            case .mentions: String(localized: "Mentions")
            case .tasks: String(localized: "Tasks")
            case .approvals: String(localized: "Approvals")
            }
        }
    }

    @State private var section: Filter = .approvals
    @State private var items: [ItemDetail] = []

    /// Tarefas: Itens do tipo tarefa e planos com linhas em aberto.
    private var tasks: [ItemDetail] {
        items.filter { it in
            if it.kindId == "task" { return true }
            guard let plan = it.plan else { return false }
            return plan.sections.flatMap(\.lines).contains { !$0.done }
        }
    }

    private func count(_ s: Filter) -> Int {
        switch s {
        case .mentions: model.mentions.count
        case .tasks: tasks.count
        case .approvals: model.pendingCount
        }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                pills

                switch section {
                case .approvals:
                    budgetAlerts
                    if pendingByAgent.isEmpty {
                        allClear
                    } else {
                        ForEach(pendingByAgent, id: \.0.id) { agent, requests in
                            batch(agent: agent, requests: requests)
                        }
                    }
                    if !resolved.isEmpty {
                        VStack(alignment: .leading, spacing: 10) {
                            SectionHeader(title: String(localized: "Resolved"))
                            VStack(spacing: 0) {
                                ForEach(resolved) { r in
                                    Button { model.push(.request(r.id)) } label: { ResolvedRow(request: r) }
                                        .buttonStyle(.plain)
                                    if r.id != resolved.last?.id { Divider().padding(.leading, 52).opacity(0.5) }
                                }
                            }
                            .background(Palette.surface, in: .rect(cornerRadius: 20, style: .continuous))
                        }
                    }
                case .mentions:
                    if model.mentions.isEmpty {
                        InkEmptyState(pose: .zen, title: String(localized: "Nobody mentioned you"))
                    }
                    ForEach(model.mentions) { m in
                        Button { model.go(.space(m.spaceId)) } label: { MentionRow(mention: m) }
                            .buttonStyle(.plain)
                    }
                case .tasks:
                    if tasks.isEmpty { InkEmptyState(pose: .zen, title: String(localized: "No open tasks")) }
                    ForEach(tasks) { it in
                        Button { model.go(.item(it.id)) } label: { TaskRow(item: it) }
                            .buttonStyle(.plain)
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .frame(maxWidth: 720)
            .frame(maxWidth: .infinity)
        }
        .scrollEdgeEffectStyle(.soft, for: .top)
        .background(NightBackdrop())
        .navigationTitle("Activity")
        .task(id: model.revision) { items = model.core.items() }
    }

    private var pills: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            GlassEffectContainer(spacing: 8) {
                HStack(spacing: 8) {
                    ForEach(Filter.allCases) { s in
                        let on = s == section
                        Button {
                            withAnimation(.snappy) { section = s }
                        } label: {
                            HStack(spacing: 6) {
                                Text(s.label).font(.subheadline.weight(on ? .semibold : .medium))
                                let n = count(s)
                                if n > 0 {
                                    Text("\(n)").font(.caption.weight(.bold)).monospacedDigit()
                                        .foregroundStyle(on ? Palette.action : .white)
                                        .padding(.horizontal, 6).frame(minWidth: 20, minHeight: 20)
                                        .background(on ? Color.white : Palette.action, in: .capsule)
                                }
                            }
                            .padding(.horizontal, 2)
                        }
                        .buttonStyle(.glass)
                        .tint(on ? Palette.action : nil)
                        .accessibilityAddTraits(on ? .isSelected : [])
                    }
                }
                .padding(.vertical, 2)
            }
        }
        .sensoryFeedback(.selection, trigger: section)
    }

    private func batch(agent: Persona, requests: [AgentRequestDto]) -> some View {
        let profile = model.agentProfile(agent.id)
        let approvable = requests.filter { $0.status == .pending }
        return VStack(alignment: .leading, spacing: 12) {
            rowLayout {
                AgentAvatar(persona: agent, size: 44, state: .waiting, budgetFraction: profile.flatMap(budgetFraction))
                VStack(alignment: .leading, spacing: 2) {
                    Text(requests.count == 1 ? String(localized: "\(agent.name) wants 1 thing") : String(localized: "\(agent.name) wants \(requests.count) things"))
                        .font(.title3.weight(.semibold))
                }
                if !typeSize.isAccessibilitySize { Spacer() }
                if approvable.count > 1 {
                    Button("Approve all") { model.approveAll(agent: agent) }
                        .buttonStyle(.glassProminent)
                        .font(.subheadline.weight(.semibold))
                }
            }
            ForEach(requests) { r in
                RequestCard(request: r, onApprove: { model.approve(r) }, onDeny: { model.deny(r) })
                    .contentShape(.rect)
                    .onTapGesture { model.push(.request(r.id)) }
                    .accessibilityAction(named: String(localized: "Review")) { model.push(.request(r.id)) }
                    .transition(.asymmetric(insertion: .opacity, removal: .scale(scale: 0.9).combined(with: .opacity)))
            }
        }
        .animation(.spring(duration: 0.45), value: requests.map(\.id))
    }

    @ViewBuilder
    private var budgetAlerts: some View {
        let near = model.agents.filter { $0.nearLimit }
        ForEach(near) { a in
            rowLayout {
                BudgetRing(spent: a.budgetSpentCents ?? 0, limit: a.budgetLimitCents ?? 1, size: 46, lineWidth: 6, showsLabel: true)
                VStack(alignment: .leading, spacing: 2) {
                    Text("\(a.persona.name) is near the cap")
                        .font(.subheadline.weight(.semibold))
                        .lineLimit(typeSize.isAccessibilitySize ? 3 : 1).minimumScaleFactor(0.8)
                    Text("\(Money.format(a.budgetSpentCents ?? 0)) of \(Money.format(a.budgetLimitCents ?? 0)) this month")
                        .font(.caption.weight(.medium)).monospacedDigit().foregroundStyle(Palette.amber)
                }
                if !typeSize.isAccessibilitySize {
                    Spacer()
                    // Near the cap: the mascot squints at a coin (budget state).
                    MascotView(pose: .coin).frame(width: 64, height: 64)
                }
                Button("+" + Money.format(2_000)) {
                    if model.perform({ try model.core.raiseBudget(agentId: a.id, extraCents: 2_000) }) != nil {
                        model.show(.init(kind: .agent(a.persona), text: String(localized: "\(a.persona.name)’s limit raised by \(Money.format(2_000)) this month.")), seconds: 3)
                    }
                }
                .buttonStyle(.glass)
                .font(.subheadline.weight(.semibold))
            }
            .solidCard(radius: 20, padding: 14)
        }
    }

    private var allClear: some View {
        InkEmptyState(pose: .cheer, title: String(localized: "Nothing needs you"),
                      message: String(localized: "Your agents do what’s reversible and ask before anything else. Requests show up here, batched."),
                      size: 140)
    }

    private func budgetFraction(_ p: AgentProfile) -> Double? {
        guard let s = p.budgetSpentCents, let l = p.budgetLimitCents, l > 0 else { return nil }
        return Double(s) / Double(l)
    }
}

struct MentionRow: View {
    let mention: Mention
    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Avatar(persona: mention.entry.author, size: 40)
            VStack(alignment: .leading, spacing: 3) {
                HStack {
                    Text("\(mention.entry.author.name) mentioned you").font(.subheadline.weight(.semibold))
                    Spacer()
                    Text(RodaTime.relative(mention.entry.atMs)).font(.caption).foregroundStyle(Palette.textTertiary)
                }
                if case .message(let text, _) = mention.entry.kind {
                    Text(text).font(.subheadline).foregroundStyle(Palette.textSecondary).lineLimit(3)
                }
                Chip(text: mention.spaceTitle, symbol: "bubble.left.and.bubble.right")
            }
        }
        .solidCard(radius: 20, padding: 14)
    }
}

struct ResolvedRow: View {
    let request: AgentRequestDto
    var body: some View {
        HStack(spacing: 12) {
            AgentAvatar(persona: request.agent, size: 28, showsOwner: false)
            VStack(alignment: .leading, spacing: 1) {
                Text(request.title).font(.subheadline).lineLimit(1)
                Text(request.status == .approved ? String(localized: "Approved · done (simulated)") : String(localized: "Declined"))
                    .font(.caption).foregroundStyle(Palette.textSecondary)
            }
            Spacer()
            if let c = request.costCents { Text(Money.format(c)).font(.subheadline).monospacedDigit().foregroundStyle(Palette.textSecondary) }
            Image(systemName: request.status == .approved ? "checkmark.circle.fill" : "xmark.circle")
                .foregroundStyle(request.status == .approved ? Palette.success : Palette.textTertiary)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 11)
    }
}

struct TaskRow: View {
    let item: ItemDetail
    var body: some View {
        let lines = item.plan?.sections.flatMap(\.lines) ?? []
        let open = lines.filter { !$0.done }.count
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: item.plan == nil ? "checklist" : "map.fill")
                .foregroundStyle(item.plan == nil ? Palette.success : Palette.action)
                .frame(width: 40, height: 40)
                .background((item.plan == nil ? Palette.success : Palette.action).opacity(0.12), in: .rect(cornerRadius: 12, style: .continuous))
            VStack(alignment: .leading, spacing: 3) {
                Text(item.title).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.textPrimary)
                Text(item.plan == nil ? (item.text ?? item.origin) : String(localized: "\(open) of \(lines.count) items open"))
                    .font(.caption).foregroundStyle(Palette.textSecondary).lineLimit(2)
                Chip(text: item.spaceTitle, symbol: "bubble.left.and.bubble.right")
            }
            Spacer()
            Image(systemName: "chevron.right").font(.caption.weight(.bold)).foregroundStyle(Palette.textTertiary)
        }
        .solidCard(radius: 20, padding: 14)
    }
}
