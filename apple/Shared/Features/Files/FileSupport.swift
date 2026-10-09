import SwiftUI
import RodaCore
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
    /// The bytes of a file's version as a real file (Quick Look needs a name and type).
    static func localURL(core: RodaEngine, itemId: String, version: UInt32?, name: String) -> URL? {
        let v = version.map { "v\($0)" } ?? "latest"
        let dir = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("ZoenFiles", isDirectory: true)
            .appendingPathComponent(itemId, isDirectory: true)
            .appendingPathComponent(v, isDirectory: true)
        let url = dir.appendingPathComponent(name.isEmpty ? "file" : name)
        if version != nil, FileManager.default.fileExists(atPath: url.path) { return url }
        guard let bytes = try? core.fileBytes(itemId: itemId, version: version) else { return nil }
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        do { try bytes.write(to: url, options: .atomic) } catch { return nil }
        return url
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
        let req = QLThumbnailGenerator.Request(fileAt: url, size: CGSize(width: 240, height: 240), scale: 2, representationTypes: .thumbnail)
        guard let rep = try? await QLThumbnailGenerator.shared.generateBestRepresentation(for: req) else { return nil }
        #if canImport(UIKit)
        return rep.uiImage.pngData()
        #else
        guard let tiff = rep.nsImage.tiffRepresentation, let bmp = NSBitmapImageRep(data: tiff) else { return nil }
        return bmp.representation(using: .png, properties: [:])
        #endif
    }

    /// Adds files a person picked to a Space: Markdown becomes a page, the rest files.
    @MainActor
    static func importFiles(_ urls: [URL], into spaceId: String, model: AppModel) async -> [String] {
        var ids: [String] = []
        for url in urls {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            guard let data = try? Data(contentsOf: url) else { continue }
            let name = url.lastPathComponent
            let ext = url.pathExtension.lowercased()
            if ["md", "markdown"].contains(ext), let md = String(data: data, encoding: .utf8) {
                if let it = model.perform({ try model.core.pageImportMarkdown(spaceId: spaceId, path: name, markdown: md) }) {
                    ids.append(it.id)
                }
                continue
            }
            let thumb = await thumbnail(for: url)
            if let it = model.perform({ try model.core.fileAdd(spaceId: spaceId, path: name, name: name, mime: mime(for: url), bytes: data, thumbnail: thumb) }) {
                ids.append(it.id)
            }
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
            image = PlatformImage(data: data)
        }
    }
}

#if canImport(UIKit)
/// Quick Look inside a screen (any format iOS can show).
struct QuickLookView: UIViewControllerRepresentable {
    let url: URL

    func makeCoordinator() -> Source { Source(url: url) }

    func makeUIViewController(context: Context) -> QLPreviewController {
        let ql = QLPreviewController()
        ql.dataSource = context.coordinator
        return ql
    }

    func updateUIViewController(_ ql: QLPreviewController, context: Context) {
        if context.coordinator.url != url {
            context.coordinator.url = url
            ql.reloadData()
        }
    }

    @MainActor
    final class Source: NSObject, QLPreviewControllerDataSource {
        var url: URL
        init(url: URL) { self.url = url }
        func numberOfPreviewItems(in controller: QLPreviewController) -> Int { 1 }
        func previewController(_ controller: QLPreviewController, previewItemAt index: Int) -> QLPreviewItem { url as NSURL }
    }
}

/// Full-screen Quick Look with Markup; a saved markup comes back as new bytes.
@MainActor
final class MarkupPresenter: NSObject, QLPreviewControllerDataSource, QLPreviewControllerDelegate {
    static let shared = MarkupPresenter()
    private var url: URL?
    private var onSave: ((URL) -> Void)?

    func present(url: URL, onSave: @escaping (URL) -> Void) {
        self.url = url
        self.onSave = onSave
        let ql = QLPreviewController()
        ql.dataSource = self
        ql.delegate = self
        guard var top = UIApplication.shared.connectedScenes
            .compactMap({ ($0 as? UIWindowScene)?.keyWindow?.rootViewController }).first else { return }
        while let p = top.presentedViewController { top = p }
        top.present(ql, animated: true)
    }

    func numberOfPreviewItems(in controller: QLPreviewController) -> Int { 1 }

    func previewController(_ controller: QLPreviewController, previewItemAt index: Int) -> QLPreviewItem {
        (url ?? URL(fileURLWithPath: "/")) as NSURL
    }

    nonisolated func previewController(_ controller: QLPreviewController, editingModeFor previewItem: QLPreviewItem) -> QLPreviewItemEditingMode {
        .createCopy
    }

    nonisolated func previewController(_ controller: QLPreviewController, didSaveEditedCopyOf previewItem: QLPreviewItem, at modifiedContentsURL: URL) {
        // Quick Look removes the copy after this returns: keep our own.
        let keep = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + "-" + modifiedContentsURL.lastPathComponent)
        try? FileManager.default.copyItem(at: modifiedContentsURL, to: keep)
        Task { @MainActor in self.onSave?(keep) }
    }
}
#else
struct QuickLookView: NSViewRepresentable {
    let url: URL
    func makeNSView(context: Context) -> QLPreviewView {
        let v = QLPreviewView(frame: .zero, style: .normal)!
        v.previewItem = url as NSURL
        return v
    }
    func updateNSView(_ v: QLPreviewView, context: Context) {
        if (v.previewItem as? NSURL) as URL? != url { v.previewItem = url as NSURL }
    }
}
#endif
