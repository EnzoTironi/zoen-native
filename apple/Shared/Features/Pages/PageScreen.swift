import SwiftUI
import RodaCore
import os

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
    @State private var saveState = SaveState.idle
    @State private var applyTask: Task<Void, Never>?
    @State private var commitTask: Task<Void, Never>?
    @State private var showVersions = false
    @State private var linkPrompt = false
    @State private var linkText = ""
    @State private var savedBump = 0
    @State private var isNew = false
    /// What Share sends (refreshed on load and save, not on every keystroke).
    @State private var markdown = ""

    enum SaveState: Equatable { case idle, editing, saved, syncing, failed, readOnly }

    var body: some View {
        VStack(spacing: 0) {
            if let item { header(item) }
            if let saveError {
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: "exclamationmark.circle")
                    Text(saveError).frame(maxWidth: .infinity, alignment: .leading)
                    if canEdit { Button("Try again") { commitNow() } }
                }
                .font(.caption)
                .foregroundStyle(Palette.danger)
                .padding(.horizontal, 22).padding(.vertical, 10)
                .accessibilityIdentifier("page.saveError")
            }
            PageTextView(controller: controller, editable: canEdit, autofocus: isNew && canEdit) { openLink() }
                .overlay(alignment: .topLeading) { placeholder }
            #if os(macOS)
            FormatBar(controller: controller) { openLink() }
                .frame(maxWidth: 720)
                .padding(.bottom, 8)
                .disabled(!canEdit)
            #endif
        }
        .frame(maxWidth: 760)
        .frame(maxWidth: .infinity)
        .background(Palette.background.ignoresSafeArea())
        .navigationTitle(item?.title ?? String(localized: "Page"))
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItem(placement: .principal) { savePill }
            ToolbarItemGroup(placement: .primaryAction) {
                Button { if !canEdit || commitNow() { showVersions = true } } label: { Label("Versions", systemImage: "clock.arrow.circlepath") }
                    .accessibilityIdentifier("page.versions")
                if !markdown.isEmpty {
                    ShareLink(item: markdown, preview: SharePreview(item?.title ?? "Page")) { Label("Share", systemImage: "square.and.arrow.up") }
                }
            }
        }
        .sheet(isPresented: $showVersions) {
            if let item { NavigationStack { VersoesView(item: item) } .zoenSheet([.medium, .large]) }
        }
        .alert("Link", isPresented: $linkPrompt) {
            TextField("Address", text: $linkText)
                #if os(iOS)
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                #endif
                .autocorrectionDisabled()
            Button("Cancel", role: .cancel) {}
            Button("OK") { if canEdit { controller.setLink(linkText) } }
                .disabled(!canEdit)
        } message: {
            Text("Leave empty to remove the link.")
        }
        .sensoryFeedback(.success, trigger: savedBump)
        .task { load(initial: true) }
        .onChange(of: model.revision) { reloadIfIdle() }
        .onChange(of: scenePhase) { _, phase in if phase != .active { controller.finishComposition(); commitNow() } }
        .onDisappear { controller.finishComposition(); commitNow() }
        .onAppear {
            controller.onEdit = { edited() }
        }
    }

    // MARK: parts

    private func header(_ item: ItemDetail) -> some View {
        let last = item.versions.max { $0.number < $1.number }
        let who = last.map { $0.author.isMe ? String(localized: "you") : $0.author.name } ?? item.createdBy.name
        return HStack(spacing: 8) {
            Image(systemName: "doc.richtext").foregroundStyle(Palette.action)
            Text(item.spaceTitle).fontWeight(.medium)
            Text("·")
            Text("v\(item.version) · \(who) · \(RodaTime.relative(last?.atMs ?? 0))")
                .contentTransition(.numericText())
            Spacer()
        }
        .font(.caption)
        .foregroundStyle(Palette.textSecondary)
        .padding(.horizontal, 22)
        .padding(.top, 6)
        .animation(.spring(duration: 0.4), value: item.version)
    }

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
        guard let page = try? model.core.page(itemId: itemId) else { return }
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
    }

    /// What the core has for each block (to send only what changed).
    @State private var synced: [String: PageBlockDto] = [:]
    @State private var syncedOrder: [String] = []

    private func edited() {
        guard canEdit else { return }
        editId = UUID().uuidString
        pendingEdit = true
        dirty = true
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
        let start = ContinuousClock.now
        defer { log.debug("apply \(changed.count) blocks in \(ContinuousClock.now - start, privacy: .public)") }
        do {
            let receipt = try model.core.pageApplyFrom(itemId: itemId, mutationId: editId, editContext: editContext, order: order, changed: changed)
            guard controller.load(receipt.page.blocks, keepSelection: true) else { return false }
            pendingEdit = false
            accept(receipt.page)
            saveError = receipt.page.saveError
            return true
        } catch {
            failed(error)
            return false
        }
    }

    @discardableResult private func commitNow() -> Bool {
        applyTask?.cancel()
        commitTask?.cancel()
        guard dirty else { log.debug("commit: nothing to save"); return true }
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
        controller.editable = canEdit
        if !canEdit { updateSaveState(page); return }
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
        controller.editable = canEdit
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
            controller.editable = canEdit
        }
        model.show(.init(kind: .error, text: message))
    }

    private func openLink() {
        guard canEdit else { return }
        linkText = controller.currentLink ?? ""
        linkPrompt = true
    }
}
