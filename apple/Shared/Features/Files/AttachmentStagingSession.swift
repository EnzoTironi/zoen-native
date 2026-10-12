import Foundation
import Darwin

/// A live process holds the lease. The next process removes only unlocked sessions.
final class AttachmentStagingSession: @unchecked Sendable {
    let directory: URL
    private let lease: Int32
    private static let marker = Data("zoen-attachment-session-v1".utf8)

    init(root: URL) throws {
        let manager = FileManager.default
        try manager.createDirectory(at: root, withIntermediateDirectories: true,
                                    attributes: [.posixPermissions: 0o700])
        try Self.sweep(root)
        let id = UUID().uuidString
        let preparing = root.appendingPathComponent("preparing-" + id, isDirectory: true)
        let published = root.appendingPathComponent("session-" + id, isDirectory: true)
        try manager.createDirectory(at: preparing, withIntermediateDirectories: false,
                                    attributes: [.posixPermissions: 0o700])
        var descriptor: Int32 = -1
        do {
            var owned = preparing
            var values = URLResourceValues()
            values.isExcludedFromBackup = true
            try owned.setResourceValues(values)
            #if os(iOS)
            try manager.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: preparing.path)
            #endif
            descriptor = preparing.appendingPathComponent(".lease").withUnsafeFileSystemRepresentation {
                guard let path = $0 else { return -1 }
                return Darwin.open(path, O_RDWR | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC | O_EXLOCK | O_NONBLOCK, 0o600)
            }
            guard descriptor >= 0 else { throw Self.posixError() }
            let written = Self.marker.withUnsafeBytes { Darwin.write(descriptor, $0.baseAddress, $0.count) }
            guard written == Self.marker.count else { throw Self.posixError() }
            // A sweeper cannot see this session until its lease is already held.
            try manager.moveItem(at: preparing, to: published)
            directory = published
            lease = descriptor
        } catch {
            if descriptor >= 0 { Darwin.close(descriptor) }
            try? manager.removeItem(at: preparing)
            throw error
        }
    }

    deinit { Darwin.close(lease) }

    private static func posixError() -> POSIXError {
        POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
    }

    private static func sweep(_ root: URL) throws {
        let manager = FileManager.default
        let entries = try manager.contentsOfDirectory(at: root,
            includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        for entry in entries {
            guard entry.lastPathComponent.hasPrefix("session-"),
                  UUID(uuidString: String(entry.lastPathComponent.dropFirst(8))) != nil,
                  let values = try? entry.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey]),
                  values.isDirectory == true, values.isSymbolicLink != true else { continue }
            let descriptor: Int32 = entry.appendingPathComponent(".lease").withUnsafeFileSystemRepresentation {
                guard let path = $0 else { return -1 }
                return Darwin.open(path, O_RDWR | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC | O_EXLOCK)
            }
            guard descriptor >= 0 else { continue }
            defer { Darwin.close(descriptor) }
            var metadata = stat()
            guard fstat(descriptor, &metadata) == 0,
                  metadata.st_mode & S_IFMT == S_IFREG, metadata.st_uid == getuid(),
                  metadata.st_size == marker.count else { continue }
            var contents = Data(count: marker.count)
            let count = contents.withUnsafeMutableBytes { Darwin.read(descriptor, $0.baseAddress, $0.count) }
            guard count == marker.count, contents == marker else { continue }
            try? manager.removeItem(at: entry)
        }
    }
}
