import SwiftUI
import ZoenCore

/// A file: seen with Quick Look; marked up on iPhone/iPad, saved as a new version.
struct FileScreen: View {
    @Environment(AppModel.self) private var model
    let itemId: String
    var onClose: (() -> Void)? = nil

    @State private var item: ItemDetail?
    @State private var file: AttachmentFile?
    @State private var previewFailure: String?
    @State private var showVersions = false
    @State private var saving = false
    @State private var savedBump = 0

    var body: some View {
        VStack(spacing: 0) {
            if let item { info(item) }
            ZStack {
                if let file {
                    QuickLookView(file: file)
                        .clipShape(.rect(cornerRadius: 18, style: .continuous))
                        .padding(.horizontal, 12)
                        .transition(.opacity)
                } else if let previewFailure {
                    ContentUnavailableView("Could not open file", systemImage: "doc", description: Text(previewFailure))
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
            .animation(.spring(duration: 0.4), value: file?.id)
        }
        .background(Palette.background.ignoresSafeArea())
        .navigationTitle(item?.title ?? String(localized: "File"))
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .livePageChrome(onClose: onClose, status: { EmptyView() }) {
            #if os(iOS)
            if let file {
                Button { markup(file) } label: { Label("Markup", systemImage: "pencil.tip.crop.circle") }
                    .accessibilityIdentifier("file.markup")
            }
            #endif
            Button { showVersions = true } label: { Label("Versions", systemImage: "clock.arrow.circlepath") }
            if let file { ShareLink(item: file.url) { Label("Share", systemImage: "square.and.arrow.up") } }
        }
        .sheet(isPresented: $showVersions) {
            if let item { NavigationStack { VersoesView(item: item) } .zoenSheet([.medium, .large]) }
        }
        .sensoryFeedback(.success, trigger: savedBump)
        .task(id: model.revision) { await reload() }
        .onDisappear { file?.discard(); file = nil }
    }

    private func info(_ item: ItemDetail) -> some View {
        let last = item.versions.max { $0.number < $1.number }
        let who = last.map { $0.author.isMe ? String(localized: "you") : $0.author.name } ?? item.createdBy.name
        return HStack(spacing: 10) {
            FileThumb(item: item, size: 30)
            VStack(alignment: .leading, spacing: 1) {
                Text(item.spaceTitle).font(.caption.weight(.medium)).foregroundStyle(Palette.textSecondary)
                Text("v\(item.version) · \(who) · \(ByteCountFormatter.string(fromByteCount: Int64(clamping: item.file?.bytes ?? 0), countStyle: .file))")
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

    private func reload() async {
        let fresh = try? model.core.item(itemId: itemId)
        let changed = fresh?.version != item?.version || file == nil
        if fresh?.version != item?.version { file?.discard(); file = nil; previewFailure = nil }
        item = fresh
        guard changed, let f = fresh?.file, f.ready else { return }
        do {
            let loaded = try await FileSupport.localFile(core: model.core, itemId: itemId, version: fresh?.version)
            do { try Task.checkCancellation() }
            catch { loaded?.discard(); throw error }
            file = loaded
            previewFailure = nil
        } catch is CancellationError {
        } catch {
            file?.discard()
            file = nil
            previewFailure = (error as? CoreError)?.message ?? error.localizedDescription
        }
    }

    #if os(iOS)
    private func markup(_ file: AttachmentFile) {
        Haptics.open()
        MarkupPresenter.present(file: file) { edited in
            Task { @MainActor in
                defer { edited.discard() }
                saving = true
                defer { saving = false }
                do {
                    let thumb = await FileSupport.thumbnail(for: edited.url)
                    let core = model.core
                    let version = try await Task.detached(priority: .userInitiated) {
                        let data = try AttachmentFiles.read(edited.url)
                        return try core.fileNewVersion(itemId: itemId, bytes: data, thumbnail: thumb,
                                                       note: String(localized: "Marked up")).version
                    }.value
                    model.refresh()
                    savedBump += 1
                    model.show(.init(kind: .info, text: String(localized: "Saved as version \(version).")), seconds: 3)
                } catch {
                    model.show(.init(kind: .error, text: (error as? CoreError)?.message ?? error.localizedDescription))
                }
            }
        } onFailure: { message in
            model.show(.init(kind: .error, text: message))
        }
    }
    #endif
}
