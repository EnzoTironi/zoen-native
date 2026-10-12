import Foundation
import CryptoKit
import RodaCore

struct PageEditorDraft: Codable {
    let mutationId: String
    let editContext: String
    let content: [DraftBlock]
    let baseline: [DraftBlock]

    init(mutationId: String, editContext: String, blocks: [PageBlockDto], baseline: [PageBlockDto]) {
        self.mutationId = mutationId
        self.editContext = editContext
        content = blocks.map(DraftBlock.init)
        self.baseline = baseline.map(DraftBlock.init)
    }

    var blocks: [PageBlockDto] { content.map(\.block) }
    var base: [PageBlockDto] { baseline.map(\.block) }
    var changed: [PageBlockDto] {
        let previous = Dictionary(base.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return blocks.filter { previous[$0.id] != $0 }
    }
}

struct DraftBlock: Codable {
    let id: String
    let kind: String
    let level: UInt32
    let indent: UInt32
    let number: UInt32
    let checked: Bool
    let lang: String
    let url: String
    let alt: String
    let text: String
    let spans: [DraftSpan]

    init(_ block: PageBlockDto) {
        id = block.id; kind = block.kind; level = block.level; indent = block.indent
        number = block.number; checked = block.checked; lang = block.lang
        url = block.url; alt = block.alt; text = block.text
        spans = block.spans.map(DraftSpan.init)
    }

    var block: PageBlockDto {
        PageBlockDto(id: id, kind: kind, level: level, indent: indent, number: number,
                     checked: checked, lang: lang, url: url, alt: alt, text: text, spans: spans.map(\.span))
    }
}

struct DraftSpan: Codable {
    let start: UInt32
    let end: UInt32
    let key: String
    let value: String

    init(_ span: TextSpanDto) { start = span.start; end = span.end; key = span.key; value = span.value }
    var span: TextSpanDto { TextSpanDto(start: start, end: end, key: key, value: value) }
}

/// A separate encrypted checkpoint for each open editor, including unfinished IME text.
@MainActor
final class PageDraftStore {
    struct Recovered {
        let url: URL
        let draft: PageEditorDraft
    }

    struct Unreadable: LocalizedError {
        let url: URL
        let fingerprint: Data?
        let reason: String
        var errorDescription: String? { reason }
    }

    enum Failure: LocalizedError {
        case unavailable
        case invalid
        case tooLarge

        var errorDescription: String? {
            switch self {
            case .unavailable: String(localized: "Could not unlock the page draft on this device.")
            case .invalid: String(localized: "The page draft could not be read. Your saved versions are still available.")
            case .tooLarge: String(localized: "This page is too large to save on this device.")
            }
        }
    }

    private static var active: [String: String] = [:]
    private static let maximumBytes = 8 * 1024 * 1024
    private let session = UUID().uuidString
    private let directory: URL
    private let key: SymmetricKey
    private let keyBytes: Data
    private let keyName: String
    private let vault: any SecretVault
    private let scope: Data
    private var file: URL { directory.appendingPathComponent(session + ".draft") }

    init(actor: String, space: String, item: String, vault: any SecretVault, root: URL? = nil) throws {
        guard !actor.isEmpty, !space.isEmpty, !item.isEmpty else { throw Failure.unavailable }
        scope = try JSONEncoder().encode(["zoen.page.editor.v1", actor, space, item])
        let digest = SHA256.hash(data: scope).map { String(format: "%02x", $0) }.joined()
        let actorDigest = SHA256.hash(data: Data(actor.utf8)).map { String(format: "%02x", $0) }.joined()
        let actorDirectory = (root ?? Self.defaultRoot).appendingPathComponent(actorDigest, isDirectory: true)
        directory = actorDirectory.appendingPathComponent(digest, isDirectory: true)
        self.vault = vault
        keyName = "zoen-page-editor-" + actorDigest
        if let stored = vault.load(key: keyName) {
            guard stored.count == 32 else { throw Failure.unavailable }
            keyBytes = stored
            key = SymmetricKey(data: stored)
        } else {
            // An existing checkpoint must never be replaced with a different key.
            guard !Self.active.values.contains(keyName) else { throw Failure.unavailable }
            guard try !Self.hasCheckpoints(in: actorDirectory) else { throw Failure.unavailable }
            let generated = SymmetricKey(size: .bits256)
            let bytes = generated.withUnsafeBytes { Data($0) }
            guard vault.save(key: keyName, value: bytes), vault.load(key: keyName) == bytes else { throw Failure.unavailable }
            keyBytes = bytes
            key = generated
        }
        activate()
    }

    func activate() { Self.active[session] = keyName }
    func release() { Self.active.removeValue(forKey: session) }

    func save(_ draft: PageEditorDraft) throws {
        guard vault.load(key: keyName) == keyBytes else { throw Failure.unavailable }
        let plain = try JSONEncoder().encode(draft)
        guard plain.count <= Self.maximumBytes else { throw Failure.tooLarge }
        guard let sealed = try AES.GCM.seal(plain, using: key, authenticating: scope).combined else { throw Failure.invalid }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true,
                                                attributes: [.posixPermissions: 0o700])
        try sealed.write(to: file, options: [.atomic, .completeFileProtection])
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: file.path)
        guard vault.load(key: keyName) == keyBytes else { throw Failure.unavailable }
    }

    func clear() throws { try remove(file) }

    func recoverable() throws -> [Recovered] {
        guard FileManager.default.fileExists(atPath: directory.path) else { return [] }
        let urls = try FileManager.default.contentsOfDirectory(at: directory,
            includingPropertiesForKeys: [.contentModificationDateKey]).filter {
                $0.pathExtension == "draft" && Self.active[$0.deletingPathExtension().lastPathComponent] == nil
            }.sorted {
                let a = (try? $0.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
                let b = (try? $1.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
                return a == b ? $0.lastPathComponent < $1.lastPathComponent : a < b
            }
        return try urls.map { url in
            var fingerprint: Data?
            do {
                let bytes = try Data(contentsOf: url, options: .mappedIfSafe)
                fingerprint = Data(SHA256.hash(data: bytes))
                guard bytes.count <= Self.maximumBytes + 32 else { throw Failure.tooLarge }
                let plain = try AES.GCM.open(AES.GCM.SealedBox(combined: bytes), using: key, authenticating: scope)
                return Recovered(url: url, draft: try JSONDecoder().decode(PageEditorDraft.self, from: plain))
            } catch {
                throw Unreadable(url: url, fingerprint: fingerprint,
                                 reason: (error as? Failure)?.localizedDescription ?? Failure.invalid.localizedDescription)
            }
        }
    }

    func acknowledge(_ recovered: Recovered) throws {
        try removeCheckpoint(at: recovered.url)
    }

    func discard(_ unreadable: Unreadable) throws {
        guard let fingerprint = unreadable.fingerprint,
              Data(SHA256.hash(data: try Data(contentsOf: unreadable.url, options: .mappedIfSafe))) == fingerprint else {
            throw Failure.invalid
        }
        try removeCheckpoint(at: unreadable.url)
    }

    private func removeCheckpoint(at url: URL) throws {
        guard url.deletingLastPathComponent().standardizedFileURL == directory.standardizedFileURL,
              url.pathExtension == "draft",
              Self.active[url.deletingPathExtension().lastPathComponent] == nil else { throw Failure.invalid }
        try remove(url)
    }

    private func remove(_ url: URL) throws {
        if FileManager.default.fileExists(atPath: url.path) { try FileManager.default.removeItem(at: url) }
        guard !FileManager.default.fileExists(atPath: url.path) else { throw Failure.unavailable }
    }

    private static func hasCheckpoints(in directory: URL) throws -> Bool {
        do {
            let attributes = try directory.resourceValues(forKeys: [.isSymbolicLinkKey, .isDirectoryKey])
            guard attributes.isSymbolicLink != true, attributes.isDirectory == true else { throw Failure.unavailable }
            let urls = try FileManager.default.contentsOfDirectory(at: directory,
                includingPropertiesForKeys: [.isSymbolicLinkKey, .isDirectoryKey])
            for url in urls {
                if url.pathExtension == "draft" { return true }
                let values = try url.resourceValues(forKeys: [.isSymbolicLinkKey, .isDirectoryKey])
                guard values.isSymbolicLink != true else { throw Failure.unavailable }
                if values.isDirectory == true, try hasCheckpoints(in: url) { return true }
            }
            return false
        } catch let error as NSError {
            let missing = (error.domain == NSCocoaErrorDomain &&
                (error.code == NSFileReadNoSuchFileError || error.code == NSFileNoSuchFileError)) ||
                (error.domain == NSPOSIXErrorDomain && error.code == 2)
            if missing { return false }
            throw error
        }
    }

    private static var defaultRoot: URL {
        #if DEBUG
        if let path = UserDefaults.standard.string(forKey: "ZoenDatabasePath"), path.hasPrefix("/") {
            return URL(fileURLWithPath: path).deletingLastPathComponent().appendingPathComponent("PageDrafts", isDirectory: true)
        }
        #endif
        return URL.applicationSupportDirectory.appendingPathComponent("Zoen/PageDrafts", isDirectory: true)
    }
}
