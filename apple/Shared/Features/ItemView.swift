import SwiftUI
import RodaCore

/// Opens an Item in the screen made for its kind.
struct ItemView: View {
    @Environment(AppModel.self) private var model
    let itemId: String

    var body: some View {
        let kind = (try? model.core.item(itemId: itemId))?.kindId ?? ""
        switch kind {
        case "page": PageScreen(itemId: itemId)
        case "file": FileScreen(itemId: itemId)
        default: PlanItemView(itemId: itemId)
        }
    }
}

/// Tela 9: o Item (aqui, um plano). Editável; cada edição é uma versão com Desfazer.
struct PlanItemView: View {
    @Environment(AppModel.self) private var model
    let itemId: String

    @State private var item: ItemDetail?
    @State private var editing: EditingLine?
    @State private var showVersions = false
    @State private var reaction: String?
    @State private var bump = 0

    struct EditingLine: Identifiable {
        var id: String { lineId ?? "new-\(section)" }
        var lineId: String?
        var section: UInt32
        var text: String
        var reais: Double
    }

    var body: some View {
        ScrollView {
            if let item {
                VStack(alignment: .leading, spacing: 22) {
                    header(item)
                    if let plan = item.plan {
                        budgetBlock(plan)
                        if let reaction, let fin = model.agents.first(where: { $0.persona.handle == "financeiro" })?.persona ?? model.zoen {
                            AgentReaction(agent: fin, text: reaction)
                                .transition(.move(edge: .top).combined(with: .opacity))
                        }
                        ForEach(Array(plan.sections.enumerated()), id: \.offset) { idx, section in
                            sectionView(item: item, index: UInt32(idx), section: section)
                        }
                    } else if let text = item.text {
                        Text(text).font(.body).solidCard()
                    }
                    footer(item)
                }
                .padding(.horizontal, 18)
                .padding(.vertical, 12)
                .frame(maxWidth: 720)
                .frame(maxWidth: .infinity)
            }
        }
        .background(Palette.background.ignoresSafeArea())
        .navigationTitle(item?.kindLabel ?? "Item")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                if let item {
                    Button { showVersions = true } label: { Label("Versions", systemImage: "clock.arrow.circlepath") }
                    ShareLink(item: shareText(item)) { Label("Share", systemImage: "square.and.arrow.up") }
                }
            }
        }
        .sheet(item: $editing) { e in
            NavigationStack { LineEditor(editing: e) { save($0) } }
                .presentationDetents([.height(320)])
        }
        .sheet(isPresented: $showVersions) {
            if let item { NavigationStack { VersionsView(item: item) } .presentationDetents([.medium, .large]) }
        }
        .task(id: model.revision) { item = try? model.core.item(itemId: itemId) }
        .sensoryFeedback(.success, trigger: bump)
    }

    // MARK: partes

    private func header(_ item: ItemDetail) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(item.spaceTitle)
                .font(.caption.weight(.medium))
                .foregroundStyle(Palette.textSecondary)

            Text(item.title)
                .font(.editorial(34))
                .foregroundStyle(Palette.textPrimary)

            if let plan = item.plan, !plan.summary.isEmpty {
                Text(plan.summary).font(.callout).foregroundStyle(Palette.textSecondary)
            }

            HStack(spacing: 8) {
                Avatar(persona: item.createdBy, size: 22)
                Text(item.origin)
                    .font(.caption)
                    .foregroundStyle(Palette.textSecondary)
                    .lineLimit(2)
            }
            .padding(.top, 2)
        }
    }

    private func budgetBlock(_ plan: PlanDto) -> some View {
        HStack(alignment: .center, spacing: 16) {
            if let budget = plan.budgetCents {
                BudgetRing(spent: plan.totalCents, limit: budget, size: 64, lineWidth: 7, showsLabel: true)
            }
            VStack(alignment: .leading, spacing: 4) {
                Text(Money.format(plan.totalCents))
                    .font(.system(size: 30, weight: .bold, design: .rounded))
                    .monospacedDigit()
                    .contentTransition(.numericText(value: Double(plan.totalCents)))
                if let budget = plan.budgetCents {
                    let over = plan.totalCents > budget
                    Text(over ? String(localized: "\(Money.format(plan.totalCents - budget)) over the \(Money.format(budget)) cap") : String(localized: "of \(Money.format(budget)) · \(Money.format(budget - plan.totalCents)) left"))
                        .font(.subheadline)
                        .foregroundStyle(over ? Palette.danger : Palette.textSecondary)
                        .contentTransition(.numericText())
                } else {
                    Text("estimated total").font(.subheadline).foregroundStyle(Palette.textSecondary)
                }
            }
            Spacer()
        }
        .solidCard()
        .animation(.spring(duration: 0.5), value: plan.totalCents)
    }

    private func sectionView(item: ItemDetail, index: UInt32, section: PlanSectionDto) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionHeader(title: section.title, trailing: Money.format(section.lines.reduce(0) { $0 + $1.costCents }))
            VStack(spacing: 0) {
                ForEach(section.lines) { line in
                    PlanLineRow(line: line,
                                request: item.linkedRequests.first { $0.lineId == line.id },
                                onToggle: { toggle(line) },
                                onEdit: { editing = EditingLine(lineId: line.id, section: index, text: line.text, reais: Double(line.costCents) / 100) },
                                onRemove: { remove(line) })
                    Divider().padding(.leading, 48).opacity(0.5)
                }
                Button {
                    editing = EditingLine(lineId: nil, section: index, text: "", reais: 0)
                } label: {
                    Label("Add", systemImage: "plus").font(.subheadline.weight(.medium))
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.vertical, 12).padding(.horizontal, 14)
                }
                .buttonStyle(.plain)
                .foregroundStyle(Palette.action)
            }
            .background(Palette.surface, in: .rect(cornerRadius: 20, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).strokeBorder(Palette.hairline.opacity(0.7), lineWidth: 0.5))
        }
    }

    private func footer(_ item: ItemDetail) -> some View {
        Button { showVersions = true } label: {
            HStack(spacing: 10) {
                Image(systemName: "clock.arrow.circlepath")
                VStack(alignment: .leading, spacing: 2) {
                    Text("\(item.versions.count) versions · nothing is lost").font(.subheadline.weight(.semibold))
                    Text("Every change is a signed event in the Space’s log").font(.caption).foregroundStyle(Palette.textSecondary)
                }
                Spacer()
                Image(systemName: "chevron.right").font(.caption.weight(.bold)).foregroundStyle(Palette.textTertiary)
            }
            .foregroundStyle(Palette.textPrimary)
            .solidCard(radius: 18, padding: 14)
        }
        .buttonStyle(.plain)
        .padding(.top, 4)
    }

    // MARK: ações (todas pelo núcleo)

    private func toggle(_ line: PlanLineDto) {
        let out = model.perform { try model.core.togglePlanLine(itemId: itemId, lineId: line.id) }
        if out != nil { bump += 1 }
        apply(out)
    }

    private func remove(_ line: PlanLineDto) {
        apply(model.perform { try model.core.removePlanLine(itemId: itemId, lineId: line.id) })
    }

    private func save(_ e: EditingLine) {
        let cents = Int64((e.reais * 100).rounded())
        if let lineId = e.lineId {
            apply(model.perform { try model.core.editPlanLine(itemId: itemId, lineId: lineId, text: e.text, costCents: cents) })
        } else {
            apply(model.perform { try model.core.addPlanLine(itemId: itemId, sectionIndex: e.section, text: e.text, costCents: cents) })
        }
    }

    private func apply(_ out: EditOutcome?) {
        guard let out else { return }
        withAnimation(.spring(duration: 0.45)) { item = out.item }
        if let r = model.handleEdit(out) {
            withAnimation(.spring(duration: 0.5)) { reaction = r }
        }
    }

    private func shareText(_ item: ItemDetail) -> String {
        guard let plan = item.plan else { return item.text ?? item.title }
        var s = "\(plan.title)\n\(plan.summary)\n"
        for sec in plan.sections {
            s += "\n\(sec.title)\n"
            for l in sec.lines { s += "\(l.done ? "✓" : "•") \(l.text) — \(Money.format(l.costCents))\n" }
        }
        s += String(localized: "\nTotal: \(Money.format(plan.totalCents))")
        if let b = plan.budgetCents { s += String(localized: " of \(Money.format(b))") }
        return s + String(localized: "\n\nMade in Zoen")
    }
}

struct PlanLineRow: View {
    let line: PlanLineDto
    let request: AgentRequestDto?
    var onToggle: () -> Void
    var onEdit: () -> Void
    var onRemove: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 12) {
            Button(action: onToggle) {
                Image(systemName: line.done ? "checkmark.circle.fill" : "circle")
                    .font(.system(size: 22))
                    .foregroundStyle(line.done ? Palette.success : Palette.textTertiary)
                    .contentTransition(.symbolEffect(.replace))
            }
            .buttonStyle(.plain)
            .accessibilityLabel(line.done ? String(localized: "Reopen") : String(localized: "Complete"))

            VStack(alignment: .leading, spacing: 3) {
                Text(line.text)
                    .font(.body)
                    .foregroundStyle(line.done ? Palette.textSecondary : Palette.textPrimary)
                    .strikethrough(line.done, color: Palette.textTertiary)
                if let request {
                    RequestBadge(request: request)
                }
            }
            Spacer(minLength: 8)
            Text(line.costCents == 0 ? String(localized: "free") : Money.format(line.costCents))
                .font(.callout.weight(.medium))
                .monospacedDigit()
                .foregroundStyle(line.costCents == 0 ? Palette.textTertiary : Palette.textPrimary)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .contentShape(.rect)
        .onTapGesture(perform: onEdit)
        .contextMenu {
            Button("Edit", systemImage: "pencil", action: onEdit)
            Button(line.done ? String(localized: "Reopen") : String(localized: "Complete"), systemImage: "checkmark.circle", action: onToggle)
            Button("Remove", systemImage: "trash", role: .destructive, action: onRemove)
        }
    }
}

struct RequestBadge: View {
    let request: AgentRequestDto
    var body: some View {
        HStack(spacing: 4) {
            AgentAvatar(persona: request.agent, size: 14, showsOwner: false)
            Text(label).lineLimit(1)
        }
        .font(.caption.weight(.medium))
        .foregroundStyle(color)
    }

    var label: String {
        switch request.status {
        case .pending: String(localized: "\(request.agent.name) asked to do this · in Activity")
        case .stale: String(localized: "Request out of date: the line changed")
        case .approved: String(localized: "Done by \(request.agent.name)")
        case .denied: String(localized: "You declined")
        }
    }

    var color: Color {
        switch request.status {
        case .pending: Palette.amber
        case .stale: Palette.amber
        case .approved: Palette.success
        case .denied: Palette.textTertiary
        }
    }
}

struct AgentReaction: View {
    let agent: Persona
    let text: String
    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            AgentAvatar(persona: agent, size: 28, showsOwner: false)
            VStack(alignment: .leading, spacing: 2) {
                Text(agent.name).font(.caption.weight(.semibold)).foregroundStyle(Color(hex: agent.tintHex))
                Text(text).font(.subheadline).foregroundStyle(Palette.textPrimary)
            }
            Spacer(minLength: 0)
        }
        .padding(12)
        .background(Color(hex: agent.tintHex).opacity(0.09), in: .rect(cornerRadius: 16, style: .continuous))
    }
}

struct LineEditor: View {
    @State var editing: ItemView.EditingLine
    var onSave: (ItemView.EditingLine) -> Void
    @Environment(\.dismiss) private var dismiss
    @FocusState private var focus: Bool

    var body: some View {
        Form {
            Section("Item") {
                TextField("E.g. Kayak tour", text: $editing.text)
                    .focused($focus)
            }
            Section("Cost") {
                TextField("Amount", value: $editing.reais, format: .currency(code: AppLocale.currencyCode).locale(AppLocale.locale))
                    #if os(iOS)
                    .keyboardType(.decimalPad)
                    #endif
            }
        }
        .navigationTitle(editing.lineId == nil ? String(localized: "Add") : String(localized: "Edit line"))
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Cancel", role: .cancel) { dismiss() } }
            ToolbarItem(placement: .confirmationAction) {
                Button("Save") { onSave(editing); dismiss() }
                    .disabled(editing.text.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .onAppear { focus = true }
    }
}

/// Tela 10: Versões. Restaurar é reversível (vira uma versão nova).
struct VersionsView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let item: ItemDetail

    var body: some View {
        List {
            Section {
                ForEach(item.versions) { v in
                    HStack(alignment: .top, spacing: 12) {
                        Avatar(persona: v.author, size: 30)
                        VStack(alignment: .leading, spacing: 3) {
                            HStack {
                                Text("v\(v.number)").font(.subheadline.weight(.bold)).monospacedDigit()
                                if v.number == item.version { Text("current").font(.caption2.weight(.bold)).foregroundStyle(Palette.success) }
                                if v.isUndo { Image(systemName: "arrow.uturn.backward").font(.caption).foregroundStyle(Palette.textSecondary) }
                                Spacer()
                                Text(RodaTime.relative(v.atMs)).font(.caption).foregroundStyle(Palette.textTertiary)
                            }
                            Text("\(v.author.isMe ? "Você" : v.author.name) · \(v.note)").font(.subheadline).foregroundStyle(Palette.textSecondary)
                            if let t = v.totalCents { Text(Money.format(t)).font(.caption.weight(.medium)).monospacedDigit() }
                        }
                    }
                    .padding(.vertical, 4)
                    .swipeActions {
                        if v.number != item.version {
                            Button("Restore") { restore(v) }.tint(Palette.action)
                        }
                    }
                    .contextMenu {
                        if v.number != item.version { Button("Restore this version", systemImage: "clock.arrow.circlepath") { restore(v) } }
                    }
                }
            } footer: {
                Text("Restoring deletes nothing: it creates a new version equal to the chosen one. Swipe to restore.")
            }
        }
        .navigationTitle("Versions")
        .toolbar { ToolbarItem(placement: .confirmationAction) { Button("OK") { dismiss() } } }
    }

    private func restore(_ v: VersionDto) {
        if model.perform({ try model.core.restoreVersion(itemId: item.id, version: v.number) }) != nil {
            model.show(.init(kind: .info, text: String(localized: "Version \(v.number) restored.")), seconds: 3)
            dismiss()
        }
    }
}
