import SwiftUI
import RodaCore

/// Opens an Item in the screen made for its kind.
struct ItemView: View {
    @Environment(AppModel.self) private var model
    let itemId: String
    var onClose: (() -> Void)? = nil

    var body: some View {
        let kind = (try? model.core.item(itemId: itemId))?.kindId ?? ""
        switch kind {
        case "page": PageScreen(itemId: itemId, onClose: onClose)
        case "file": FileScreen(itemId: itemId, onClose: onClose)
        default: PlanItemView(itemId: itemId, onClose: onClose)
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
    @State var editing: PlanItemView.EditingLine
    var onSave: (PlanItemView.EditingLine) -> Bool
    /// Editing an existing line: the bottom row offers [trash] [Update], Things-style.
    var onRemove: (() -> Bool)? = nil
    var allowsChanges = true
    @Environment(\.dismiss) private var dismiss
    @FocusState private var focus: Bool

    private var parsedAmount: Int64? { MoneyInput.cents(from: editing.amount, locale: AppLocale.locale) }
    private var validAmount: Bool { parsedAmount.map { $0 >= 0 } ?? false }
    private var canSave: Bool { allowsChanges && !editing.text.trimmingCharacters(in: .whitespaces).isEmpty && validAmount }

    var body: some View {
        Form {
            Section("Item") {
                TextField("E.g. Kayak tour", text: $editing.text)
                    .focused($focus)
                    .accessibilityIdentifier("line-editor-text")
                    .sheetItem(0)
            }
            Section("Cost") {
                TextField("Amount", text: $editing.amount)
                    .accessibilityIdentifier("line-editor-amount")
                    #if os(iOS)
                    .keyboardType(.numbersAndPunctuation)
                    #endif
                    .sheetItem(1)
                if !validAmount {
                    if let parsedAmount, parsedAmount < 0 {
                        Text("Cost cannot be negative.")
                            .font(.caption).foregroundStyle(Palette.danger)
                    } else {
                        Text("Enter a valid amount with up to two decimal places.")
                            .font(.caption).foregroundStyle(Palette.danger)
                    }
                }
            }
        }
        .navigationTitle(editing.lineId == nil ? String(localized: "Add") : String(localized: "Edit line"))
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Cancel", role: .cancel) { dismiss() } }
            if onRemove == nil {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { if onSave(editing) { dismiss() } }
                        .disabled(!canSave)
                }
            }
        }
        .safeAreaInset(edge: .bottom) {
            if let onRemove {
                ConfirmMorphRow(
                    primaryTitle: "Update",
                    primaryEnabled: canSave,
                    primary: { if onSave(editing) { Haptics.commit(); dismiss() } },
                    destructiveLabel: "Remove item",
                    confirmTitle: "Remove item",
                    doneTitle: "Removed",
                    confirm: { allowsChanges && onRemove() },
                    completion: { dismiss() },
                    idPrefix: "line-editor"
                )
                .disabled(!allowsChanges)
                .padding(.horizontal, 18)
                .padding(.bottom, 8)
                .sheetItem(2)
            }
        }
        // The keyboard comes up once the sheet has landed: one smooth rise instead of two.
        .task {
            guard onRemove == nil else { return }
            try? await Task.sleep(for: .seconds(0.38))
            focus = true
        }
    }
}

/// Tela 10: Versões. Restaurar é reversível (vira uma versão nova).
struct VersionsView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let item: ItemDetail
    var allowsRestore = true
    @State private var planBinding: PlanBinding = .loading

    private enum PlanBinding {
        case loading
        case ready(String)
        case failed
    }

    /// Hash captured when the sheet opens. Restore never reads `planPage` again.
    private var capturedPlanHash: String? {
        if case .ready(let hash) = planBinding { return hash }
        return nil
    }

    private var planRestoreAvailable: Bool {
        item.kindId != "plan" || capturedPlanHash != nil
    }

    var body: some View {
        List {
            Section {
                if item.kindId == "plan" {
                    if case .loading = planBinding {
                        HStack {
                            Spacer()
                            ProgressView()
                                .accessibilityIdentifier("legacy-version.hash.loading")
                            Spacer()
                        }
                    } else if case .failed = planBinding {
                        VStack(alignment: .leading, spacing: 6) {
                            Text("Couldn't open this page")
                                .font(.callout)
                                .foregroundStyle(Palette.danger)
                            Button("Try again") { Task { await loadPlanHash() } }
                                .accessibilityIdentifier("legacy-version.hash.retry")
                        }
                        .accessibilityIdentifier("legacy-version.hash.error")
                    }
                }
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
                        #if os(macOS)
                        if v.number != item.version, planRestoreAvailable {
                            Button("Restore", systemImage: "clock.arrow.circlepath") { restore(v) }
                                .accessibilityIdentifier("legacy-version.restore.\(v.number)")
                                .disabled(!allowsRestore)
                        }
                        #endif
                    }
                    .padding(.vertical, 4)
                    .swipeActions {
                        if v.number != item.version, planRestoreAvailable {
                            Button("Restore") { restore(v) }.tint(Palette.action).disabled(!allowsRestore)
                        }
                    }
                    .contextMenu {
                        if v.number != item.version, planRestoreAvailable {
                            Button("Restore this version", systemImage: "clock.arrow.circlepath") { restore(v) }.disabled(!allowsRestore)
                        }
                    }
                }
            } footer: {
                #if os(macOS)
                Text("Restoring deletes nothing: it creates a new version equal to the chosen one.")
                #else
                Text("Restoring deletes nothing: it creates a new version equal to the chosen one. Swipe to restore.")
                #endif
            }
        }
        .navigationTitle("Versions")
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                Button("OK") { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
        }
        #if os(macOS)
        .frame(minWidth: 560, minHeight: 480)
        .onExitCommand { dismiss() }
        #endif
        .task { await loadPlanHash() }
    }

    private func loadPlanHash() async {
        guard item.kindId == "plan" else { return }
        if capturedPlanHash != nil { return }
        planBinding = .loading
        do {
            let hash = try model.core.planPage(itemId: item.id).contentHash
            if Task.isCancelled { return }
            if hash.isEmpty { planBinding = .failed } else { planBinding = .ready(hash) }
        } catch {
            if Task.isCancelled { return }
            planBinding = .failed
        }
    }

    private func restore(_ v: VersionDto) {
        guard allowsRestore else { return }
        if item.kindId == "plan" {
            guard let hash = capturedPlanHash else {
                model.show(.init(kind: .error, text: String(localized: "Couldn't open this page")), seconds: 4)
                return
            }
            do {
                _ = try model.core.restorePlanVersion(itemId: item.id, version: v.number, observedHash: hash)
                model.refresh()
                model.show(.init(kind: .info, text: String(localized: "Version \(v.number) restored.")), seconds: 3)
                dismiss()
            } catch let error as CoreError {
                model.refresh()
                if case .Stale = error {
                    model.show(.init(kind: .info, text: error.message), seconds: 4)
                } else {
                    model.show(.init(kind: .error, text: error.message))
                }
            } catch {
                model.refresh()
                model.show(.init(kind: .error, text: error.localizedDescription))
            }
            return
        }
        if model.perform({ try model.core.restoreVersion(itemId: item.id, version: v.number) }) != nil {
            model.show(.init(kind: .info, text: String(localized: "Version \(v.number) restored.")), seconds: 3)
            dismiss()
        }
    }
}
