import SwiftUI
import ClientCore
import QuickLookThumbnailing
import UniformTypeIdentifiers

#if canImport(UIKit)
import UIKit
import QuickLook
#else
import AppKit
import Quartz
#endif

/// Files on disk for Quick Look, and previews made on this device.
enum FileSupport {
    enum Failure: LocalizedError {
        case historicalPreviewUnavailable

        var errorDescription: String? {
            String(localized: "This older file version cannot be previewed on this device yet.")
        }
    }

    /// The viewer keeps the owner alive until Quick Look releases its file.
    static func localFile(core: Client, itemId: String, version: UInt32?) async throws -> AttachmentFile? {
        let work = Task.detached(priority: .userInitiated) {
            let item = try core.item(itemId: itemId)
            // Old versions lack a byte-count DTO. Do not allocate them without a bounded core read.
            guard version == nil || version == item.version else { throw Failure.historicalPreviewUnavailable }
            guard let info = item.file else { return nil as AttachmentFile? }
            guard info.bytes <= UInt64(AttachmentFiles.maximumBytes) else { throw AttachmentFiles.Failure.tooLarge }
            guard let bytes = try core.fileBytes(itemId: itemId, version: item.version) else { return nil }
            try Task.checkCancellation()
            return try AttachmentFiles.stage(bytes, name: info.name)
        }
        return try await withTaskCancellationHandler {
            let file = try await work.value
            do { try Task.checkCancellation() }
            catch { file?.discard(); throw error }
            return file
        } onCancel: { work.cancel() }
    }

    static func mime(for url: URL) -> String {
        UTType(filenameExtension: url.pathExtension)?.preferredMIMEType ?? "application/octet-stream"
    }

    static func symbol(mime: String) -> String {
        if mime.hasPrefix("image/") { return "photo.fill" }
        if mime.hasPrefix("video/") { return "film.fill" }
        if mime.hasPrefix("audio/") { return "waveform" }
        if mime == "application/pdf" { return "doc.richtext.fill" }
        if mime.contains("spreadsheet") || mime == "text/csv" { return "tablecells.fill" }
        if mime.contains("presentation") { return "rectangle.on.rectangle.angled" }
        if mime.contains("zip") { return "doc.zipper" }
        return "doc.fill"
    }

    /// A PNG preview made by Quick Look on this device (travels with the file).
    static func thumbnail(for url: URL) async -> Data? {
        guard let size = try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize,
              Int64(size) <= AttachmentFiles.maximumBytes else { return nil }
        do {
            if let bytes = try await Task.detached(priority: .userInitiated, operation: {
                try AttachmentFiles.imageThumbnail(url)
            }).value { return bytes }
        } catch { return nil }
        let req = QLThumbnailGenerator.Request(fileAt: url, size: CGSize(width: 240, height: 240), scale: 2, representationTypes: .thumbnail)
        guard let rep = try? await QLThumbnailGenerator.shared.generateBestRepresentation(for: req) else { return nil }
        guard rep.cgImage.width <= 512, rep.cgImage.height <= 512 else { return nil }
        #if canImport(UIKit)
        let data = rep.uiImage.pngData()
        #else
        guard let tiff = rep.nsImage.tiffRepresentation, let bmp = NSBitmapImageRep(data: tiff) else { return nil }
        let data = bmp.representation(using: .png, properties: [:])
        #endif
        guard let data, data.count <= 2 << 20 else { return nil }
        return data
    }

    @MainActor @discardableResult
    static func sendAttachment(_ file: AttachmentFile, into spaceId: String, model: AppModel) async throws -> String {
        guard let use = file.beginUse() else { throw CocoaError(.fileNoSuchFile) }
        defer { use.close() }
        let thumb = await thumbnail(for: file.url)
        let core = model.core
        // Once accepted, the core owns encrypted chunks and its durable upload queue.
        let id = try await Task.detached(priority: .userInitiated) {
            let bytes = try AttachmentFiles.read(file.url)
            return try core.fileAdd(spaceId: spaceId, path: file.name, name: file.name,
                                    mime: mime(for: file.url), bytes: bytes, thumbnail: thumb).id
        }.value
        model.refresh()
        return id
    }

    /// Adds files a person picked to a Space: Markdown becomes a page, the rest files.
    /// All the Markdown goes in at once, so a folder of notes is sealed and sent as a batch.
    @MainActor
    static func importFiles(_ urls: [URL], into spaceId: String, model: AppModel) async -> [String] {
        var ids: [String] = []
        var pages: [MarkdownFileDto] = []
        do {
            let files = try await AttachmentFiles.importSelection(urls)
            defer { files.forEach { $0.discard() } }
            for file in files {
                let ext = file.url.pathExtension.lowercased()
                if ["md", "markdown"].contains(ext) {
                    let data = try await Task.detached(priority: .userInitiated) { try AttachmentFiles.read(file.url) }.value
                    if let md = String(data: data, encoding: .utf8) {
                        pages.append(MarkdownFileDto(path: file.name, markdown: md))
                        continue
                    }
                }
                ids.append(try await sendAttachment(file, into: spaceId, model: model))
            }
            if !pages.isEmpty, let made = model.perform({ try model.core.pagesImportMarkdown(spaceId: spaceId, files: pages) }) {
                ids.append(contentsOf: made.map(\.id))
            }
        } catch {
            model.show(.init(kind: .error, text: (error as? CoreError)?.message ?? error.localizedDescription))
        }
        return ids
    }
}

/// A file's preview image, or its kind's symbol.
struct FileThumb: View {
    @Environment(AppModel.self) private var model
    let item: ItemDetail
    var size: CGFloat = 34
    @State private var image: PlatformImage?

    var body: some View {
        Group {
            if let image {
                #if canImport(UIKit)
                Image(uiImage: image).resizable().scaledToFill()
                #else
                Image(nsImage: image).resizable().scaledToFill()
                #endif
            } else {
                Image(systemName: item.kindId == "page" ? "doc.richtext" : FileSupport.symbol(mime: item.file?.mime ?? ""))
                    .font(.system(size: size * 0.55))
                    .foregroundStyle(item.kindId == "page" ? Palette.action : Palette.textSecondary)
            }
        }
        .frame(width: size, height: size)
        .clipShape(.rect(cornerRadius: size * 0.22, style: .continuous))
        .task(id: "\(item.id)-\(item.version)") {
            guard item.file?.hasThumbnail == true, let data = try? model.core.fileThumbnail(itemId: item.id) else { return }
            let bounded = await Task.detached(priority: .userInitiated) { AttachmentFiles.imageThumbnail(data) }.value
            image = bounded.flatMap { PlatformImage(data: $0) }
        }
    }
}

#if canImport(UIKit)
/// Quick Look inside a screen (any format iOS can show).
struct QuickLookView: UIViewControllerRepresentable {
    let file: AttachmentFile

    func makeCoordinator() -> Source { Source(file: file) }

    func makeUIViewController(context: Context) -> QLPreviewController {
        let ql = QLPreviewController()
        ql.dataSource = context.coordinator
        return ql
    }

    func updateUIViewController(_ ql: QLPreviewController, context: Context) {
        if context.coordinator.file.id != file.id {
            let oldUse = context.coordinator.use
            context.coordinator.file = file
            context.coordinator.use = file.beginUse()
            ql.reloadData()
            oldUse?.close()
        }
    }

    static func dismantleUIViewController(_ ql: QLPreviewController, coordinator: Source) {
        ql.dataSource = nil
        ql.reloadData()
        coordinator.use?.close()
        coordinator.use = nil
    }

    @MainActor
    final class Source: NSObject, QLPreviewControllerDataSource {
        var file: AttachmentFile
        var use: AttachmentFile.Use?
        init(file: AttachmentFile) { self.file = file; use = file.beginUse() }
        func numberOfPreviewItems(in controller: QLPreviewController) -> Int { use == nil ? 0 : 1 }
        func previewController(_ controller: QLPreviewController, previewItemAt index: Int) -> QLPreviewItem { file.url as NSURL }
    }
}

/// Full-screen Quick Look with Markup; a saved markup comes back as new bytes.
@MainActor
final class MarkupPresenter: NSObject, @preconcurrency QLPreviewControllerDataSource, @preconcurrency QLPreviewControllerDelegate {
    nonisolated let file: AttachmentFile
    private var use: AttachmentFile.Use?
    private let onSave: @MainActor @Sendable (AttachmentFile) -> Void
    private let onFailure: @MainActor @Sendable (String) -> Void

    private init(file: AttachmentFile, onSave: @escaping @MainActor @Sendable (AttachmentFile) -> Void,
                 onFailure: @escaping @MainActor @Sendable (String) -> Void) {
        self.file = file
        self.use = file.beginUse()
        self.onSave = onSave
        self.onFailure = onFailure
    }

    static func present(file: AttachmentFile, onSave: @escaping @MainActor @Sendable (AttachmentFile) -> Void,
                        onFailure: @escaping @MainActor @Sendable (String) -> Void) {
        guard var top = UIApplication.shared.connectedScenes
            .compactMap({ ($0 as? UIWindowScene)?.keyWindow?.rootViewController }).first else { return }
        while let p = top.presentedViewController { top = p }
        let source = MarkupPresenter(file: file, onSave: onSave, onFailure: onFailure)
        guard source.use != nil else { return }
        let ql = OwnedMarkupController()
        ql.owner = source
        ql.dataSource = source
        ql.delegate = source
        top.present(ql, animated: true)
    }

    func numberOfPreviewItems(in controller: QLPreviewController) -> Int { 1 }

    func previewController(_ controller: QLPreviewController, previewItemAt index: Int) -> QLPreviewItem {
        file.url as NSURL
    }

    func previewControllerDidDismiss(_ controller: QLPreviewController) {
        controller.dataSource = nil
        close()
    }

    func close() {
        use?.close()
        use = nil
    }

    nonisolated func previewController(_ controller: QLPreviewController, editingModeFor previewItem: QLPreviewItem) -> QLPreviewItemEditingMode {
        .createCopy
    }

    nonisolated func previewController(_ controller: QLPreviewController, didSaveEditedCopyOf previewItem: QLPreviewItem, at modifiedContentsURL: URL) {
        // Quick Look removes its copy after this callback returns.
        do {
            let saved = try AttachmentFiles.stage([modifiedContentsURL])[0]
            Task { @MainActor in self.onSave(saved) }
        } catch {
            let message = error.localizedDescription
            Task { @MainActor in self.onFailure(message) }
        }
    }
}

private final class OwnedMarkupController: QLPreviewController {
    var owner: MarkupPresenter?
    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        if isBeingDismissed || presentingViewController == nil {
            dataSource = nil
            owner?.close()
            owner = nil
        }
    }
}
#else
struct QuickLookView: NSViewRepresentable {
    let file: AttachmentFile
    func makeNSView(context: Context) -> QLPreviewView {
        let v = OwnedPreviewView(frame: .zero, style: .normal)!
        v.owner = file
        v.use = file.beginUse()
        if v.use != nil { v.previewItem = file.url as NSURL }
        return v
    }
    func updateNSView(_ v: QLPreviewView, context: Context) {
        guard let owned = v as? OwnedPreviewView, owned.owner?.id != file.id else { return }
        let oldUse = owned.use
        owned.owner = file
        owned.use = file.beginUse()
        v.previewItem = owned.use == nil ? nil : file.url as NSURL
        oldUse?.close()
    }
    static func dismantleNSView(_ v: QLPreviewView, coordinator: ()) {
        v.previewItem = nil
        v.close()
        guard let owned = v as? OwnedPreviewView else { return }
        owned.use?.close()
        owned.use = nil
        owned.owner = nil
    }
}

private final class OwnedPreviewView: QLPreviewView {
    var owner: AttachmentFile?
    var use: AttachmentFile.Use?
}
#endif
