import Foundation
import ImageIO
import UniformTypeIdentifiers
import Darwin

/// One privately staged file. Discard closes the review; active users delay removal.
final class AttachmentFile: Identifiable, @unchecked Sendable {
    let id: UUID
    let url: URL
    let name: String
    let byteCount: Int64
    private let directory: URL
    private let session: AttachmentStagingSession?
    private let lock = NSLock()
    private var activeUses = 0
    private var discarded = false

    fileprivate init(id: UUID, directory: URL, name: String, byteCount: Int64, session: AttachmentStagingSession?) {
        self.id = id
        self.directory = directory
        self.name = name
        self.byteCount = byteCount
        self.session = session
        self.url = directory.appendingPathComponent(name)
    }

    deinit { try? FileManager.default.removeItem(at: directory) }

    func beginUse() -> Use? {
        lock.withLock {
            guard !discarded else { return nil }
            activeUses += 1
            return Use(file: self)
        }
    }

    func discard() {
        let remove = lock.withLock {
            discarded = true
            return activeUses == 0
        }
        if remove { try? FileManager.default.removeItem(at: directory) }
    }

    private func endUse() {
        let remove = lock.withLock {
            activeUses -= 1
            return discarded && activeUses == 0
        }
        if remove { try? FileManager.default.removeItem(at: directory) }
    }

    final class Use: @unchecked Sendable {
        private let lock = NSLock()
        private var file: AttachmentFile?
        fileprivate init(file: AttachmentFile) { self.file = file }
        func close() {
            let owner = lock.withLock {
                let owner = file
                file = nil
                return owner
            }
            owner?.endUse()
        }
        deinit { close() }
    }
}

struct AttachmentDraft: Identifiable {
    let id = UUID()
    let files: [AttachmentFile]
    func discard() { files.forEach { $0.discard() } }
}

/// Bounded reads and protected temporary copies for import, review and Quick Look.
enum AttachmentFiles {
    private static let liveSession: Result<AttachmentStagingSession, any Error> = Result {
        try AttachmentStagingSession(root: FileManager.default.temporaryDirectory
            .appendingPathComponent("ZoenAttachmentSessions", isDirectory: true))
    }
    // UniFFI and the core also copy the complete file during encryption.
    static let maximumBytes: Int64 = 32 << 20
    static let maximumSelectionBytes: Int64 = 64 << 20
    static let maximumSelection = 10
    static let maximumImagePixels = 64_000_000
    static let maximumImageDimension = 16_384

    enum Failure: LocalizedError {
        case empty, tooLarge, selectionTooLarge, notAFile, tooMany, imageTooLarge, invalidImage

        var errorDescription: String? {
            switch self {
            case .empty: String(localized: "This file is empty.")
            case .tooLarge: String(localized: "Choose a file of 32 MB or less.")
            case .selectionTooLarge: String(localized: "Choose files totaling 64 MB or less.")
            case .notAFile: String(localized: "Choose a file instead of a folder.")
            case .tooMany: String(localized: "Choose up to 10 files at a time.")
            case .imageTooLarge: String(localized: "Choose a photo of 64 megapixels or less.")
            case .invalidImage: String(localized: "This photo could not be opened.")
            }
        }
    }

    static func safeName(_ proposed: String) -> String {
        let leaf = proposed.replacingOccurrences(of: "\\", with: "/")
            .components(separatedBy: "/").last ?? ""
        var name = String(leaf.unicodeScalars.filter {
            !CharacterSet.controlCharacters.contains($0)
        }).trimmingCharacters(in: .whitespacesAndNewlines)
        if name.isEmpty || name == "." || name == ".." { name = "file" }
        while name.utf8.count > 200 { name.removeLast() }
        return name
    }

    static func read(_ url: URL, limit: Int64 = maximumBytes) throws -> Data {
        let (handle, size) = try openRegularFile(url)
        defer { try? handle.close() }
        guard size <= limit else { throw Failure.tooLarge }
        var bytes = Data()
        while let part = try handle.read(upToCount: 1 << 20), !part.isEmpty {
            try Task.checkCancellation()
            guard Int64(bytes.count) + Int64(part.count) <= limit else { throw Failure.tooLarge }
            bytes.append(part)
        }
        guard !bytes.isEmpty else { throw Failure.empty }
        return bytes
    }

    static func stage(_ urls: [URL], root: URL? = nil) throws -> [AttachmentFile] {
        guard urls.count <= maximumSelection else { throw Failure.tooMany }
        var staged: [AttachmentFile] = []
        var remaining = maximumSelectionBytes
        for url in urls {
            try Task.checkCancellation()
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            let file = try copy(url, limit: min(maximumBytes, remaining), root: root)
            remaining -= file.byteCount
            staged.append(file)
        }
        return staged
    }

    static func importSelection(_ urls: [URL]) async throws -> [AttachmentFile] {
        let work = Task.detached(priority: .userInitiated) { try stage(urls) }
        return try await withTaskCancellationHandler {
            let files = try await work.value
            do { try Task.checkCancellation() }
            catch { files.forEach { $0.discard() }; throw error }
            return files
        } onCancel: { work.cancel() }
    }

    static func stage(_ bytes: Data, name: String, root: URL? = nil) throws -> AttachmentFile {
        try Task.checkCancellation()
        guard !bytes.isEmpty else { throw Failure.empty }
        guard Int64(bytes.count) <= maximumBytes else { throw Failure.tooLarge }
        let (id, directory, url, session) = try location(name: name, root: root)
        do {
            #if os(iOS)
            try bytes.write(to: url, options: [.atomic, .completeFileProtection])
            #else
            try bytes.write(to: url, options: .atomic)
            #endif
            try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
            try validateImage(url)
            try Task.checkCancellation()
            return AttachmentFile(id: id, directory: directory, name: url.lastPathComponent, byteCount: Int64(bytes.count), session: session)
        } catch {
            try? FileManager.default.removeItem(at: directory)
            throw error
        }
    }

    private static func location(name: String, root: URL?) throws -> (UUID, URL, URL, AttachmentStagingSession?) {
        let session = root == nil ? try liveSession.get() : nil
        guard let base = root ?? session?.directory else { throw CocoaError(.fileWriteUnknown) }
        try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700])
        let id = UUID()
        let directory = base.appendingPathComponent(id.uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700])
        do {
            var owned = directory
            var values = URLResourceValues()
            values.isExcludedFromBackup = true
            try owned.setResourceValues(values)
            #if os(iOS)
            try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: directory.path)
            #endif
            return (id, directory, directory.appendingPathComponent(safeName(name)), session)
        } catch {
            try? FileManager.default.removeItem(at: directory)
            throw error
        }
    }

    private static func copy(_ source: URL, limit: Int64, root: URL?) throws -> AttachmentFile {
        let (input, initialSize) = try openRegularFile(source)
        defer { try? input.close() }
        if initialSize > limit {
            throw initialSize > maximumBytes ? Failure.tooLarge : Failure.selectionTooLarge
        }
        let (id, directory, url, session) = try location(name: source.lastPathComponent, root: root)
        do {
            var attributes: [FileAttributeKey: Any] = [.posixPermissions: 0o600]
            #if os(iOS)
            attributes[.protectionKey] = FileProtectionType.complete
            #endif
            guard FileManager.default.createFile(atPath: url.path, contents: nil, attributes: attributes) else {
                throw CocoaError(.fileWriteUnknown)
            }
            let output = try FileHandle(forWritingTo: url)
            defer { try? output.close() }
            var size: Int64 = 0
            while let part = try input.read(upToCount: 1 << 20), !part.isEmpty {
                try Task.checkCancellation()
                size += Int64(part.count)
                guard size <= limit else {
                    throw size > maximumBytes ? Failure.tooLarge : Failure.selectionTooLarge
                }
                try output.write(contentsOf: part)
            }
            guard size > 0 else { throw Failure.empty }
            try validateImage(url)
            try Task.checkCancellation()
            return AttachmentFile(id: id, directory: directory, name: url.lastPathComponent, byteCount: size, session: session)
        } catch {
            try? FileManager.default.removeItem(at: directory)
            throw error
        }
    }

    private static func openRegularFile(_ url: URL) throws -> (FileHandle, Int64) {
        try Task.checkCancellation()
        let descriptor: Int32 = url.withUnsafeFileSystemRepresentation {
            guard let path = $0 else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        }
        guard descriptor >= 0 else {
            if errno == ELOOP { throw Failure.notAFile }
            throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
        }
        var metadata = stat()
        guard fstat(descriptor, &metadata) == 0 else {
            let error = POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
            Darwin.close(descriptor)
            throw error
        }
        guard metadata.st_mode & S_IFMT == S_IFREG, metadata.st_size >= 0 else {
            Darwin.close(descriptor)
            throw Failure.notAFile
        }
        return (FileHandle(fileDescriptor: descriptor, closeOnDealloc: true), metadata.st_size)
    }

    static func validateImage(_ url: URL) throws {
        let type = UTType(filenameExtension: url.pathExtension)
        guard let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary),
              CGImageSourceGetType(source) != nil else {
            if type?.conforms(to: .image) == true && type != .svg { throw Failure.invalidImage }
            return
        }
        try validateImage(source)
    }

    private static func validateImage(_ source: CGImageSource) throws {
        guard let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = (properties[kCGImagePropertyPixelWidth] as? NSNumber)?.intValue,
              let height = (properties[kCGImagePropertyPixelHeight] as? NSNumber)?.intValue,
              width > 0, height > 0 else { throw Failure.invalidImage }
        guard width <= maximumImageDimension, height <= maximumImageDimension,
              width <= maximumImagePixels / height else { throw Failure.imageTooLarge }
    }

    static func imageThumbnail(_ url: URL) throws -> Data? {
        try validateImage(url)
        guard let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary),
              CGImageSourceGetType(source) != nil else { return nil }
        return imageThumbnail(source)
    }

    static func imageThumbnail(_ bytes: Data) -> Data? {
        guard bytes.count <= 2 << 20,
              let source = CGImageSourceCreateWithData(bytes as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
              (try? validateImage(source)) != nil else { return nil }
        return imageThumbnail(source)
    }

    private static func imageThumbnail(_ source: CGImageSource) -> Data? {
        let options: [CFString: Any] = [kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true, kCGImageSourceThumbnailMaxPixelSize: 480,
            kCGImageSourceShouldCacheImmediately: true]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return nil }
        let bytes = NSMutableData()
        guard let output = CGImageDestinationCreateWithData(bytes, UTType.png.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(output, image, nil)
        guard CGImageDestinationFinalize(output) else { return nil }
        return bytes as Data
    }
}
