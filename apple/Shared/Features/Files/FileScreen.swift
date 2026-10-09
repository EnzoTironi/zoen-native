import SwiftUI
import RodaCore

/// A file: seen with Quick Look; marked up on iPhone/iPad, saved as a new version.
struct FileScreen: View {
    @Environment(AppModel.self) private var model
    let itemId: String

    @State private var item: ItemDetail?
    @State private var url: URL?
    @State private var showVersions = false
    @State private var saving = false
    @State private var savedBump = 0

    var body: some View {
        VStack(spacing: 0) {
            if let item { info(item) }
            ZStack {
                if let url {
                    QuickLookView(url: url)
                        .clipShape(.rect(cornerRadius: 18, style: .continuous))
                        .padding(.horizontal, 12)
                        .transition(.opacity)
                } else if let f = item?.file {
                    arriving(f)
                }
                if saving {
                    ProgressView("Saving a new version…")
                        .padding(18)
                        .background(.regularMaterial, in: .rect(cornerRadius: 16, style: .continuous))
                }
            }
            .frame(maxHeight: .infinity)
            .animation(.spring(duration: 0.4), value: url)
        }
        .background(Palette.background.ignoresSafeArea())
        .navigationTitle(item?.title ?? String(localized: "File"))
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                #if os(iOS)
                if let url {
                    Button { markup(url) } label: { Label("Markup", systemImage: "pencil.tip.crop.circle") }
                        .accessibilityIdentifier("file.markup")
                }
                #endif
                Button { showVersions = true } label: { Label("Versions", systemImage: "clock.arrow.circlepath") }
                if let url { ShareLink(item: url) { Label("Share", systemImage: "square.and.arrow.up") } }
            }
        }
        .sheet(isPresented: $showVersions) {
            if let item { NavigationStack { VersoesView(item: item) } .zoenSheet([.medium, .large]) }
        }
        .sensoryFeedback(.success, trigger: savedBump)
        .task(id: model.revision) { reload() }
    }

    private func info(_ item: ItemDetail) -> some View {
        let last = item.versions.max { $0.number < $1.number }
        let who = last.map { $0.author.isMe ? String(localized: "you") : $0.author.name } ?? item.createdBy.name
        return HStack(spacing: 10) {
            FileThumb(item: item, size: 30)
            VStack(alignment: .leading, spacing: 1) {
                Text(item.spaceTitle).font(.caption.weight(.medium)).foregroundStyle(Palette.textSecondary)
                Text("v\(item.version) · \(who) · \(ByteCountFormatter.string(fromByteCount: Int64(item.file?.bytes ?? 0), countStyle: .file))")
                    .font(.caption).foregroundStyle(Palette.textTertiary)
                    .contentTransition(.numericText())
            }
            Spacer()
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 8)
    }

    private func arriving(_ f: FileDto) -> some View {
        VStack(spacing: 14) {
            ProgressView(value: Double(f.chunksHere), total: Double(max(f.chunks, 1)))
                .progressViewStyle(.circular)
                .controlSize(.large)
            Text("Arriving…").font(.headline)
            Text("\(f.chunksHere) of \(f.chunks) parts on this device").font(.subheadline).foregroundStyle(Palette.textSecondary)
        }
    }

    private func reload() {
        let fresh = try? model.core.item(itemId: itemId)
        let changed = fresh?.version != item?.version || url == nil
        item = fresh
        guard changed, let f = fresh?.file, f.ready else { return }
        url = FileSupport.localURL(core: model.core, itemId: itemId, version: fresh?.version, name: f.name)
    }

    #if os(iOS)
    private func markup(_ url: URL) {
        Haptics.open()
        MarkupPresenter.shared.present(url: url) { edited in
            Task { @MainActor in
                saving = true
                defer { saving = false }
                guard let data = try? Data(contentsOf: edited) else { return }
                let thumb = await FileSupport.thumbnail(for: edited)
                try? FileManager.default.removeItem(at: edited)
                if let it = model.perform({ try model.core.fileNewVersion(itemId: itemId, bytes: data, thumbnail: thumb, note: String(localized: "Marked up")) }) {
                    savedBump += 1
                    model.show(.init(kind: .info, text: String(localized: "Saved as version \(it.version).")), seconds: 3)
                }
            }
        }
    }
    #endif
}
