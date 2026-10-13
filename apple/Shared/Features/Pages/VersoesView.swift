import SwiftUI
import RodaCore

struct VersionHistorySheet: View {
    let item: ItemDetail
    var allowsRestore = true

    var body: some View {
        NavigationStack { VersoesView(item: item, allowsRestore: allowsRestore) }
            #if os(macOS)
            .frame(width: 560, height: 600)
            #endif
            .zoenSheet([.medium, .large])
    }
}

/// Every version of a page or file: who, when, what it looked like; restore any of them
/// (as a new version, so nothing is lost).
struct VersoesView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let item: ItemDetail
    var allowsRestore = true
    @State private var current: ItemDetail?

    private var shown: ItemDetail { current ?? item }

    var body: some View {
        List {
            Section {
                ForEach(shown.versions.sorted { $0.number > $1.number }) { v in
                    NavigationLink {
                        VersionPreview(item: shown, version: v, allowsRestore: allowsRestore) { restored in
                            current = restored
                        }
                    } label: {
                        row(v)
                    }
                    .accessibilityIdentifier("version.\(v.number)")
                }
            } footer: {
                Text("Restoring keeps everything: it adds a new version equal to the one you pick.")
            }
        }
        .navigationTitle("Versions")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                Button("OK") { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
        }
        .task(id: model.revision) { current = try? model.core.item(itemId: item.id) }
        #if os(macOS)
        .onExitCommand { dismiss() }
        #endif
    }

    private func row(_ v: VersionDto) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Avatar(persona: v.author, size: 30)
            VStack(alignment: .leading, spacing: 3) {
                HStack {
                    Text("v\(v.number)").font(.subheadline.weight(.bold)).monospacedDigit()
                    if v.number == shown.version {
                        Text("current").font(.caption2.weight(.bold)).foregroundStyle(Palette.success)
                    }
                    Spacer()
                    Text(RodaTime.relative(v.atMs)).font(.caption).foregroundStyle(Palette.textTertiary)
                }
                Text("\(v.author.isMe ? String(localized: "You") : v.author.name) · \(v.note)")
                    .font(.subheadline).foregroundStyle(Palette.textSecondary)
            }
        }
        .padding(.vertical, 4)
    }
}

/// One version, read-only, with "Restore".
struct VersionPreview: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let item: ItemDetail
    let version: VersionDto
    var allowsRestore = true
    var onRestore: (ItemDetail) -> Void = { _ in }

    @State private var controller = PageEditorController()
    @State private var file: AttachmentFile?
    @State private var previewFailure: String?
    @State private var bump = 0
    @State private var currentVersion: UInt32 = 0
    @State private var canRestore = false
    @State private var observedPlanHash: String?
    @State private var planHashFailed = false

    var body: some View {
        Group {
            if item.kindId == "page" {
                PageTextView(controller: controller, editable: false)
                    .task {
                        if let p = try? model.core.pageAt(itemId: item.id, version: version.number) {
                            controller.load(p.blocks)
                        }
                    }
            } else if let file {
                QuickLookView(file: file)
            } else if let previewFailure {
                ContentUnavailableView("Could not open file", systemImage: "doc", description: Text(previewFailure))
            } else {
                ProgressView()
                    .task {
                        do {
                            let loaded = try await FileSupport.localFile(core: model.core, itemId: item.id, version: version.number)
                            do { try Task.checkCancellation() }
                            catch { loaded?.discard(); throw error }
                            file = loaded
                        } catch is CancellationError {
                        } catch {
                            previewFailure = (error as? CoreError)?.message ?? error.localizedDescription
                        }
                    }
            }
        }
        .background(Palette.background.ignoresSafeArea())
        .onDisappear { file?.discard(); file = nil }
        .navigationTitle("v\(version.number)")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .safeAreaInset(edge: .bottom) {
            if item.kindId == "plan", observedPlanHash == nil {
                VStack(spacing: 6) {
                    if planHashFailed {
                        Text("Couldn't open this page")
                            .font(.callout)
                            .foregroundStyle(Palette.danger)
                        Button("Try again") {
                            refreshPermission()
                            loadObservedPlanHash()
                        }
                        .accessibilityIdentifier("version.hash.retry")
                    } else {
                        ProgressView()
                            .accessibilityIdentifier("version.hash.loading")
                    }
                }
                .frame(maxWidth: .infinity)
                .padding(.horizontal, 18)
                .padding(.bottom, 8)
            } else if canRestore && version.number != currentVersion {
                Button {
                    restore()
                } label: {
                    Label("Restore this version", systemImage: "clock.arrow.circlepath")
                        .font(.body.weight(.semibold))
                        .foregroundStyle(ActionGlassForeground())
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 6)
                }
                .buttonStyle(.glassProminent)
                .tint(Palette.action)
                .padding(.horizontal, 18)
                .padding(.bottom, 8)
                .accessibilityIdentifier("version.restore")
            }
        }
        .sensoryFeedback(.success, trigger: bump)
        .task(id: model.revision) {
            refreshPermission()
            loadObservedPlanHash()
        }
    }

    private func restore() {
        refreshPermission()
        guard canRestore, version.number != currentVersion else { return }
        if item.kindId == "plan" {
            guard let hash = observedPlanHash else {
                model.show(.init(kind: .error, text: String(localized: "Couldn't open this page")), seconds: 4)
                return
            }
            do {
                let it = try model.core.restorePlanVersion(itemId: item.id, version: version.number, observedHash: hash)
                model.refresh()
                bump += 1
                model.show(.init(kind: .info, text: String(localized: "Version \(version.number) is back, as version \(it.version).")), seconds: 3)
                onRestore(it)
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
        if let it = model.perform({ try model.core.restoreVersion(itemId: item.id, version: version.number) }) {
            var savedVersion = it.version
            if item.kindId == "page" {
                guard let page = try? model.core.page(itemId: item.id) else {
                    model.show(.init(kind: .error, text: String(localized: "Could not confirm restoration. Check the current page.")))
                    return
                }
                if page.unsaved || page.pendingSync || page.saveError != nil {
                    model.show(.init(kind: page.saveError == nil ? .info : .error,
                        text: page.saveError ?? String(localized: "Restoration is waiting to sync.")), seconds: 4)
                    onRestore(it)
                    dismiss()
                    return
                }
                savedVersion = page.version
            }
            bump += 1
            model.show(.init(kind: .info, text: String(localized: "Version \(version.number) is back, as version \(savedVersion).")), seconds: 3)
            onRestore(it)
            dismiss()
        }
    }

    private func refreshPermission() {
        currentVersion = (try? model.core.item(itemId: item.id).version) ?? item.version
        if item.kindId == "plan" {
            canRestore = allowsRestore && ((try? model.core.planPage(itemId: item.id).legacyEditAdmitted) ?? false)
        } else if item.kindId == "page" {
            canRestore = allowsRestore && ((try? model.core.page(itemId: item.id).canEdit) ?? false)
        } else {
            canRestore = allowsRestore
        }
    }

    /// Hash lido na abertura. Restaurar não chama `planPage` de novo para reler o hash.
    private func loadObservedPlanHash() {
        guard item.kindId == "plan", observedPlanHash == nil else { return }
        planHashFailed = false
        if let hash = try? model.core.planPage(itemId: item.id).contentHash, !hash.isEmpty {
            observedPlanHash = hash
        } else {
            planHashFailed = true
        }
    }
}
