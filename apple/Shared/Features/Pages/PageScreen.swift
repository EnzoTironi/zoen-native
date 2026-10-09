import SwiftUI
import RodaCore

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

    enum SaveState: Equatable { case idle, editing, saved }

    var body: some View {
        VStack(spacing: 0) {
            if let item { header(item) }
            PageTextView(controller: controller, editable: true, autofocus: isNew) { openLink() }
                .overlay(alignment: .topLeading) { placeholder }
            #if os(macOS)
            FormatBar(controller: controller) { openLink() }
                .frame(maxWidth: 720)
                .padding(.bottom, 8)
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
                Button { commitNow(); showVersions = true } label: { Label("Versions", systemImage: "clock.arrow.circlepath") }
                    .accessibilityIdentifier("page.versions")
                if !markdown.isEmpty {
                    ShareLink(item: markdown, preview: SharePreview(item?.title ?? "Page")) { Label("Share", systemImage: "square.and.arrow.up") }
                }
            }
        }
        .sheet(isPresented: $showVersions) {
            if let item { NavigationStack { VersoesView(item: item) } .presentationDetents([.medium, .large]) }
        }
        .alert("Link", isPresented: $linkPrompt) {
            TextField("Address", text: $linkText)
                #if os(iOS)
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                #endif
                .autocorrectionDisabled()
            Button("Cancel", role: .cancel) {}
            Button("OK") { controller.setLink(linkText) }
        } message: {
            Text("Leave empty to remove the link.")
        }
        .sensoryFeedback(.success, trigger: savedBump)
        .task { load(initial: true) }
        .onChange(of: model.revision) { reloadIfIdle() }
        .onChange(of: scenePhase) { _, phase in if phase != .active { commitNow() } }
        .onDisappear { commitNow() }
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
        controller.load(blocks, keepSelection: !initial)
        loadedVersion = page.version
        markdown = (try? model.core.pageMarkdown(itemId: itemId)) ?? ""
        if initial {
            synced = Dictionary(page.blocks.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            syncedOrder = page.blocks.map(\.id)
        }
    }

    /// What the core has for each block (to send only what changed).
    @State private var synced: [String: PageBlockDto] = [:]
    @State private var syncedOrder: [String] = []

    private func edited() {
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

    private func applyNow() {
        let blocks = controller.blocks()
        let order = blocks.map(\.id)
        let changed = blocks.filter { synced[$0.id] != $0 }
        if changed.isEmpty && order == syncedOrder { return }
        do {
            try model.core.pageApply(itemId: itemId, order: order, changed: changed)
            synced = Dictionary(blocks.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            syncedOrder = order
        } catch {
            model.show(.init(kind: .error, text: (error as? CoreError)?.message ?? error.localizedDescription))
        }
    }

    private func commitNow() {
        applyTask?.cancel()
        commitTask?.cancel()
        guard dirty else { return }
        applyNow()
        dirty = false
        if model.perform({ try model.core.pageCommit(itemId: itemId, note: "") }) == true {
            savedBump += 1
            withAnimation(.spring(duration: 0.4, bounce: 0.35)) { saveState = .saved }
            Task { @MainActor in
                try? await Task.sleep(for: .seconds(2))
                if saveState == .saved, !dirty { withAnimation(.easeOut(duration: 0.3)) { saveState = .idle } }
            }
        } else if saveState == .editing {
            withAnimation { saveState = .idle }
        }
        item = try? model.core.item(itemId: itemId)
        if let v = item?.version { loadedVersion = v }
        markdown = (try? model.core.pageMarkdown(itemId: itemId)) ?? ""
    }

    /// Someone else saved a version: show it unless this person is mid-edit.
    private func reloadIfIdle() {
        let fresh = try? model.core.item(itemId: itemId)
        item = fresh ?? item
        guard !dirty, let v = fresh?.version, v != loadedVersion else { return }
        guard let page = try? model.core.page(itemId: itemId) else { return }
        controller.load(page.blocks, keepSelection: true)
        synced = Dictionary(page.blocks.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        syncedOrder = page.blocks.map(\.id)
        loadedVersion = page.version
        markdown = (try? model.core.pageMarkdown(itemId: itemId)) ?? ""
    }

    private func openLink() {
        linkText = controller.currentLink ?? ""
        linkPrompt = true
    }
}
