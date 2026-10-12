import SwiftUI
import RodaCore
import os
#if os(macOS)
import AppKit
#else
import UIKit
#endif

private let log = Logger(subsystem: "xyz.tironi.zoen", category: "page")

/// A page: Markdown you edit as it looks. Edits stay on this device for a moment and
/// are saved as a version for everyone when you pause or leave.
struct PageScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.scenePhase) private var scenePhase
    let itemId: String

    @State private var controller = PageEditorController()
    @State private var item: ItemDetail?
    @State private var loadedVersion: UInt32 = 0
    @State private var dirty = false
    @State private var pendingEdit = false
    @State private var canEdit = false
    @State private var editContext = ""
    @State private var editId = UUID().uuidString
    @State private var saveError: String?
    @State private var draftStore: PageDraftStore?
    @State private var draftReady = false
    @State private var recoveryDraft: DraftRecovery?
    @State private var showDiscardDraft = false
    @State private var saveState = SaveState.idle
    @State private var applyTask: Task<Void, Never>?
    @State private var commitTask: Task<Void, Never>?
    @State private var versionHistory: VersionHistoryPresentation?
    @State private var linkPrompt = false
    @State private var linkText = ""
    @State private var savedBump = 0
    @State private var isNew = false
    /// What Share sends (refreshed on load and save, not on every keystroke).
    @State private var markdown = ""

    enum SaveState: Equatable { case idle, editing, saved, syncing, failed, readOnly }
    enum DraftRecovery { case orphan(PageDraftStore.Recovered), currentSession, unreadable(PageDraftStore.Unreadable) }

    private struct VersionHistoryPresentation: Identifiable {
        let id = UUID()
        let item: ItemDetail
        let allowsRestore: Bool
    }

    var body: some View {
        VStack(spacing: 0) {
            if let item { LivePageMetadata(item: item) }
            if let saveError {
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: "exclamationmark.circle")
                    Text(saveError).frame(maxWidth: .infinity, alignment: .leading)
                    if canEdit && recoveryDraft == nil { Button("Try again") { retrySave() } }
                }
                .font(.caption)
                .foregroundStyle(Palette.danger)
                .padding(.horizontal, 22).padding(.vertical, 10)
                .accessibilityIdentifier("page.saveError")
            }
            if let recoveryDraft {
                HStack {
                    if case .unreadable(let checkpoint) = recoveryDraft {
                        ShareLink(item: checkpoint.url) { Label("Keep draft file", systemImage: "square.and.arrow.up") }
                    } else {
                        Button("Copy draft text") { copyRecoveryText() }
                    }
                    Spacer()
                    Button("Discard draft", role: .destructive) { showDiscardDraft = true }
                }
                .font(.caption).padding(.horizontal, 22).padding(.bottom, 10)
                .accessibilityIdentifier("page.recoveryActions")
            }
            PageTextView(controller: controller, editable: canEdit && draftReady, autofocus: isNew && canEdit && draftReady) { openLink() }
                .overlay(alignment: .topLeading) { placeholder }
            #if os(macOS)
            FormatBar(controller: controller) { openLink() }
                .frame(maxWidth: 720)
                .padding(.bottom, 8)
                .disabled(!canEdit || !draftReady)
            #endif
        }
        .livePageSurface()
        .navigationTitle(item?.title ?? String(localized: "Page"))
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItem(placement: .principal) { savePill }
            ToolbarItemGroup(placement: .primaryAction) {
                Button {
                    let allowsRestore = canEdit && draftReady && commitNow()
                    if let item {
                        versionHistory = VersionHistoryPresentation(item: item, allowsRestore: allowsRestore)
                    }
                } label: { Label("Versions", systemImage: "clock.arrow.circlepath") }
                    .accessibilityIdentifier("page.versions")
                if !markdown.isEmpty {
                    ShareLink(item: markdown, preview: SharePreview(item?.title ?? "Page")) { Label("Share", systemImage: "square.and.arrow.up") }
                }
            }
        }
        .sheet(item: $versionHistory) { presentation in
            VersionHistorySheet(item: presentation.item, allowsRestore: presentation.allowsRestore)
        }
        .confirmationDialog("Discard this draft?", isPresented: $showDiscardDraft, titleVisibility: .visible) {
            Button("Discard draft", role: .destructive) { discardRecoveredDraft() }
        } message: {
            if case .unreadable = recoveryDraft {
                Text("Keep a copy of the encrypted draft file first. Discarding removes only this unreadable draft and keeps the current page and its saved versions.")
            } else {
                Text("Copy any text you want to keep first. Discarding opens the current page and keeps its saved versions.")
            }
        }
        .alert("Link", isPresented: $linkPrompt) {
            TextField("Address", text: $linkText)
                #if os(iOS)
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                #endif
                .autocorrectionDisabled()
            Button("Cancel", role: .cancel) {}
            Button("OK") { if canEdit && draftReady { controller.setLink(linkText) } }
                .disabled(!canEdit || !draftReady)
        } message: {
            Text("Leave empty to remove the link.")
        }
        .sensoryFeedback(.success, trigger: savedBump)
        .task { load(initial: true) }
        .onChange(of: model.revision) { reloadIfIdle() }
        .onChange(of: scenePhase) { _, phase in if phase != .active { controller.finishComposition(); commitNow() } }
        .onDisappear { controller.finishComposition(); commitNow(); draftStore?.release() }
        .onAppear {
            draftStore?.activate()
            controller.onEdit = { edited() }
        }
    }

    // MARK: parts

    @ViewBuilder private var savePill: some View {
        switch saveState {
        case .idle: EmptyView()
        case .editing:
            Label("Editing", systemImage: "pencil")
                .font(.caption.weight(.semibold))
                .foregroundStyle(Palette.textSecondary)
                .padding(.horizontal, 10).padding(.vertical, 5)
                .background(Palette.surfaceMuted, in: .capsule)
                .transition(.scale(scale: 0.8).combined(with: .opacity))
        case .saved:
            Label("Saved", systemImage: "checkmark.circle.fill")
                .font(.caption.weight(.semibold))
                .foregroundStyle(Palette.action)
                .symbolEffect(.bounce, value: savedBump)
                .padding(.horizontal, 10).padding(.vertical, 5)
                .background(Palette.action.opacity(0.12), in: .capsule)
                .transition(.scale(scale: 0.8).combined(with: .opacity))
                .accessibilityIdentifier("page.saved")
        case .syncing:
            Label("Saved on this device", systemImage: "icloud.and.arrow.up")
                .font(.caption).foregroundStyle(Palette.textSecondary)
                .accessibilityIdentifier("page.pendingSync")
        case .failed:
            Label("Couldn't save", systemImage: "exclamationmark.circle")
                .font(.caption).foregroundStyle(Palette.danger)
        case .readOnly:
            Label("Read only", systemImage: "lock")
                .font(.caption).foregroundStyle(Palette.textSecondary)
        }
    }

    @ViewBuilder private var placeholder: some View {
        if controller.isEmpty {
            Text("Title")
                .font(.system(size: 30, weight: .bold))
                .foregroundStyle(Palette.textTertiary)
                .padding(.leading, 23)
                .padding(.top, 8)
                .allowsHitTesting(false)
        }
    }

    // MARK: data

    private func load(initial: Bool) {
        item = try? model.core.item(itemId: itemId)
        guard var page = try? model.core.page(itemId: itemId) else { return }
        if pendingEdit { retrySave(); return }
        var draftError: Error?
        if initial, page.canEdit {
            do {
                if draftStore == nil {
                    draftStore = try PageDraftStore(actor: model.me?.id ?? "", space: page.spaceId, item: itemId,
                                                    vault: KeychainVault(service: "xyz.tironi.zoen.page-drafts"))
                }
                guard let draftStore else { throw PageDraftStore.Failure.unavailable }
                draftReady = true
                for recovered in try draftStore.recoverable() {
                    let draft = recovered.draft
                    do {
                        let receipt = try model.core.pageApplyFrom(itemId: itemId, mutationId: draft.mutationId,
                            editContext: draft.editContext, order: draft.blocks.map(\.id), changed: draft.changed)
                        try draftStore.acknowledge(recovered)
                        page = receipt.page
                    } catch {
                        // Keep the observed branch and the encrypted checkpoint, including after a restore fence.
                        recoveryDraft = .orphan(recovered)
                        draftReady = false
                        controller.load(draft.blocks)
                        synced = Dictionary(draft.base.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
                        syncedOrder = draft.base.map(\.id)
                        editContext = draft.editContext
                        editId = draft.mutationId
                        canEdit = page.canEdit
                        controller.editable = false
                        loadedVersion = page.version
                        pendingEdit = true
                        dirty = true
                        failed(error)
                        return
                    }
                }
            } catch {
                draftReady = false
                if let unreadable = error as? PageDraftStore.Unreadable { recoveryDraft = .unreadable(unreadable) }
                draftError = error
            }
        }
        var blocks = page.blocks
        if initial, page.version == 1, blocks.count <= 2, blocks.allSatisfy({ $0.text.isEmpty }) {
            // A new page: start on the title.
            isNew = true
            blocks = [PageBlockDto(id: blocks.first?.id ?? BlockTag.newId(), kind: "heading", level: 1, indent: 0, number: 0,
                                   checked: false, lang: "", url: "", alt: "", text: "", spans: [])]
        }
        guard controller.load(blocks, keepSelection: !initial) else { return }
        accept(page)
        markdown = (try? model.core.pageMarkdown(itemId: itemId)) ?? ""
        dirty = page.unsaved && page.canEdit
        updateSaveState(page)
        if let draftError { failed(draftError) }
        else if page.unsaved && page.canEdit && draftReady && !page.pendingSync && page.saveError == nil {
            commitTask?.cancel()
            commitTask = Task { @MainActor in
                try? await Task.sleep(for: .seconds(4))
                guard !Task.isCancelled else { return }
                commitNow()
            }
        }
    }

    /// What the core has for each block (to send only what changed).
    @State private var synced: [String: PageBlockDto] = [:]
    @State private var syncedOrder: [String] = []

    private func edited() {
        guard canEdit, recoveryDraft == nil else { return }
        editId = UUID().uuidString
        pendingEdit = true
        dirty = true
        guard checkpoint() else { return }
        if saveState != .editing { withAnimation(.spring(duration: 0.35, bounce: 0.3)) { saveState = .editing } }
        applyTask?.cancel()
        applyTask = Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(400))
            guard !Task.isCancelled else { return }
            applyNow()
        }
        commitTask?.cancel()
        commitTask = Task { @MainActor in
            try? await Task.sleep(for: .seconds(4))
            guard !Task.isCancelled else { return }
            commitNow()
        }
    }

    @discardableResult private func applyNow() -> Bool {
        guard !controller.isComposing else { return false }
        // An undo may equal the old baseline after an accepted save lost its reply.
        guard pendingEdit else { return true }
        let blocks = controller.blocks()
        let order = blocks.map(\.id)
        let changed = blocks.filter { synced[$0.id] != $0 }
        guard canEdit, !editContext.isEmpty else { return false }
        guard checkpoint() else { return false }
        let start = ContinuousClock.now
        defer { log.debug("apply \(changed.count) blocks in \(ContinuousClock.now - start, privacy: .public)") }
        do {
            let receipt = try model.core.pageApplyFrom(itemId: itemId, mutationId: editId, editContext: editContext, order: order, changed: changed)
            try draftStore?.clear()
            guard controller.load(receipt.page.blocks, keepSelection: true) else { return false }
            pendingEdit = false
            accept(receipt.page)
            saveError = receipt.page.saveError
            return true
        } catch {
            if let coreError = error as? CoreError, case .Stale = coreError {
                recoveryDraft = .currentSession
                draftReady = false
            }
            failed(error)
            return false
        }
    }

    @discardableResult private func commitNow() -> Bool {
        applyTask?.cancel()
        commitTask?.cancel()
        guard dirty else { log.debug("commit: nothing to save"); return true }
        guard draftReady || pendingEdit else { return false }
        let start = ContinuousClock.now
        defer { log.debug("commit in \(ContinuousClock.now - start, privacy: .public), now v\(loadedVersion)") }
        guard applyNow() else { return false }
        do {
            let committed = try model.core.pageCommit(itemId: itemId, note: "")
            let page = try model.core.page(itemId: itemId)
            guard controller.load(page.blocks, keepSelection: true) else { return false }
            accept(page)
            dirty = page.unsaved && page.canEdit
            if committed && !page.pendingSync && page.saveError == nil { savedBump += 1 }
            updateSaveState(page)
            item = try? model.core.item(itemId: itemId)
            markdown = (try? model.core.pageMarkdown(itemId: itemId)) ?? markdown
            return page.saveError == nil && !page.unsaved
        } catch {
            failed(error)
            return false
        }
    }

    /// Someone else saved a version: show it unless this person is mid-edit.
    private func reloadIfIdle() {
        let fresh = try? model.core.item(itemId: itemId)
        item = fresh ?? item
        guard let page = try? model.core.page(itemId: itemId) else { return }
        canEdit = page.canEdit
        controller.editable = canEdit && draftReady
        guard recoveryDraft == nil else { return }
        if canEdit && !draftReady && !pendingEdit { load(initial: true); return }
        if !canEdit { updateSaveState(page) }
        guard !controller.isComposing, !pendingEdit else { return }
        if dirty {
            let blocks = controller.blocks()
            guard blocks.map(\.id) == syncedOrder, blocks.allSatisfy({ synced[$0.id] == $0 }) else { return }
        }
        guard controller.load(page.blocks, keepSelection: true) else { return }
        accept(page)
        dirty = page.unsaved && page.canEdit
        updateSaveState(page)
        markdown = (try? model.core.pageMarkdown(itemId: itemId)) ?? markdown
        if page.unsaved && page.canEdit && !page.pendingSync && page.saveError == nil { commitNow() }
    }

    private func accept(_ page: PageDto) {
        synced = Dictionary(page.blocks.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        syncedOrder = page.blocks.map(\.id)
        editContext = page.editContext
        editId = UUID().uuidString
        canEdit = page.canEdit
        controller.editable = canEdit && draftReady
        loadedVersion = page.version
    }

    private func updateSaveState(_ page: PageDto) {
        saveError = page.saveError
        withAnimation(.spring(duration: 0.35)) {
            if page.saveError != nil { saveState = .failed }
            else if !page.canEdit { saveState = .readOnly }
            else if page.unsaved { saveState = .editing }
            else if page.pendingSync { saveState = .syncing }
            else { saveState = .saved }
        }
    }

    private func failed(_ error: Error) {
        let message = (error as? CoreError)?.message ?? error.localizedDescription
        saveError = message
        saveState = .failed
        dirty = true
        if let page = try? model.core.page(itemId: itemId) {
            canEdit = page.canEdit
            controller.editable = canEdit && draftReady
        }
        model.show(.init(kind: .error, text: message))
    }

    private func openLink() {
        guard canEdit && draftReady else { return }
        linkText = controller.currentLink ?? ""
        linkPrompt = true
    }

    @discardableResult private func checkpoint() -> Bool {
        guard recoveryDraft == nil else { return false }
        do {
            guard let draftStore, !editContext.isEmpty else { throw PageDraftStore.Failure.unavailable }
            let baseline = syncedOrder.compactMap { synced[$0] }
            try draftStore.save(PageEditorDraft(mutationId: editId, editContext: editContext,
                                                blocks: controller.blocks(), baseline: baseline))
            draftReady = true
            return true
        } catch {
            draftReady = false
            failed(error)
            return false
        }
    }

    private func copyRecoveryText() {
        let text = controller.blocks().map(\.text).joined(separator: "\n\n")
        #if os(macOS)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        #else
        UIPasteboard.general.string = text
        #endif
        model.show(.init(kind: .info, text: String(localized: "Draft text copied.")))
    }

    private func discardRecoveredDraft() {
        guard let recoveryDraft, let draftStore, !controller.isComposing else { return }
        do {
            // Verify access to the current page before deleting the explicitly discarded checkpoint.
            _ = try model.core.page(itemId: itemId)
            switch recoveryDraft {
            case .orphan(let recovered): try draftStore.acknowledge(recovered)
            case .currentSession: try draftStore.clear()
            case .unreadable(let checkpoint): try draftStore.discard(checkpoint)
            }
            self.recoveryDraft = nil
            pendingEdit = false
            dirty = false
            saveError = nil
            load(initial: true)
        } catch { failed(error) }
    }

    private func retrySave() {
        if pendingEdit { _ = checkpoint(); commitNow() }
        else if !draftReady { load(initial: true) }
        else { commitNow() }
    }
}
