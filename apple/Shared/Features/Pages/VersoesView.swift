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
    @State private var url: URL?
    @State private var bump = 0
    @State private var currentVersion: UInt32 = 0
    @State private var canRestore = false

    var body: some View {
        Group {
            if item.kindId == "page" {
                PageTextView(controller: controller, editable: false)
                    .task {
                        if let p = try? model.core.pageAt(itemId: item.id, version: version.number) {
                            controller.load(p.blocks)
                        }
                    }
            } else if let url {
                QuickLookView(url: url)
            } else {
                ProgressView()
                    .task {
                        url = FileSupport.localURL(core: model.core, itemId: item.id, version: version.number, name: item.file?.name ?? item.title)
                    }
            }
        }
        .background(Palette.background.ignoresSafeArea())
        .navigationTitle("v\(version.number)")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .safeAreaInset(edge: .bottom) {
            if canRestore && version.number != currentVersion {
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
        .task(id: model.revision) { refreshPermission() }
    }

    private func restore() {
        refreshPermission()
        guard canRestore, version.number != currentVersion else { return }
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
        if item.kindId == "page" {
            canRestore = allowsRestore && ((try? model.core.page(itemId: item.id).canEdit) ?? false)
        } else {
            canRestore = allowsRestore
        }
    }
}
