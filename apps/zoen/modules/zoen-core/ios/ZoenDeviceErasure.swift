import Foundation
import Darwin

/// A restartable operation intent, never another account store. It contains no keys or content.
struct ZoenDeviceScope: Codable, Equatable, Sendable {
  let identityId: String
  let deviceId: String
  init(identityId: String, deviceId: String) throws {
    let valid = { (value: String) in value.count == 64 && value.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) } }
    guard valid(identityId), valid(deviceId) else { throw ZoenDeviceErasure.Failure.changed }
    self.identityId = identityId
    self.deviceId = deviceId
  }
}

final class ZoenDeviceErasure {
  private static let mediaLibraryExportDirectoryName = "MediaLibraryUriExtractorDirectory"
  static var accountRoot: URL { URL.applicationSupportDirectory.appending(path: "account", directoryHint: .isDirectory) }
  private let root: URL
  init(root: URL = ZoenDeviceErasure.accountRoot) { self.root = root }
  private var marker: URL { root.appending(path: "erase-device-v1.json") }
  private struct Intent: Codable { let schemaVersion: Int; let scope: ZoenDeviceScope }
  enum Failure: Error, LocalizedError {
    case changed, incomplete
    var errorDescription: String? {
      switch self {
      case .changed: NSLocalizedString("The account changed. Open the confirmation again.", comment: "Device erasure scope changed")
      case .incomplete: NSLocalizedString("Sign-out on this device could not be completed. Try again.", comment: "Device erasure needs a retry")
      }
    }
  }
  /// `fileExists` also returns false on access failures. Only missing files count as absent.
  private static func exists(_ url: URL) throws -> Bool {
    do {
      _ = try FileManager.default.attributesOfItem(atPath: url.path)
      return true
    } catch let error as CocoaError where error.code == .fileNoSuchFile || error.code == .fileReadNoSuchFile {
      return false
    } catch { throw Failure.incomplete }
  }
  func pending() throws -> ZoenDeviceScope? {
    guard try Self.exists(marker) else { return nil }
    let attributes = try FileManager.default.attributesOfItem(atPath: marker.path)
    guard let size = attributes[.size] as? NSNumber, size.intValue <= 2048 else { throw Failure.incomplete }
    let intent = try JSONDecoder().decode(Intent.self, from: Data(contentsOf: marker))
    guard intent.schemaVersion == 1 else { throw Failure.incomplete }
    // Codable decoding must receive the same validation as a fresh bridge request.
    return try ZoenDeviceScope(identityId: intent.scope.identityId, deviceId: intent.scope.deviceId)
  }
  func begin(_ scope: ZoenDeviceScope) throws {
    if let existing = try pending(), existing != scope { throw Failure.changed }
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    var directory = root
    var values = URLResourceValues(); values.isExcludedFromBackup = true
    try directory.setResourceValues(values)
    let data = try JSONEncoder().encode(Intent(schemaVersion: 1, scope: scope))
    try data.write(to: marker, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
  }
  func complete() throws {
    if try Self.exists(marker) { try FileManager.default.removeItem(at: marker) }
    if try Self.exists(marker) { throw Failure.incomplete }
  }
  /// Only the URI returned by our own SDK58 CURRENT operation may be submitted here.
  /// Library originals are no-ops; only an exact direct SDK-export filename can be unlinked.
  static func releaseMediaLibraryExport(_ uri: String, temporaryRoot: URL = FileManager.default.temporaryDirectory) throws {
    guard let components = URLComponents(string: uri), components.scheme == "file", let url = components.url else { return }
    let root = temporaryRoot.standardizedFileURL
    let canonicalRoot = root.resolvingSymlinksInPath().standardizedFileURL
    let namespace = root.appending(path: mediaLibraryExportDirectoryName, directoryHint: .isDirectory)
    let canonicalNamespace = canonicalRoot.appending(path: mediaLibraryExportDirectoryName, directoryHint: .isDirectory)
    let rawPath = components.path
    let normalizedPath = url.standardizedFileURL.path
    let roots = [namespace.path, canonicalNamespace.path]
    guard roots.contains(where: { rawPath.hasPrefix($0 + "/") || normalizedPath.hasPrefix($0 + "/") }) else { return }
    let filename = url.lastPathComponent
    let prefix = "slowMoVideo-"
    let suffix = ".mov"
    guard components.host == nil || components.host == "",
          components.query == nil, components.fragment == nil,
          components.percentEncodedPath == rawPath,
          rawPath == normalizedPath,
          uri == URL(fileURLWithPath: rawPath).absoluteString,
          roots.contains(url.deletingLastPathComponent().path),
          filename.hasPrefix(prefix), filename.hasSuffix(suffix) else { throw Failure.incomplete }
    let identifier = String(filename.dropFirst(prefix.count).dropLast(suffix.count))
    guard UUID(uuidString: identifier)?.uuidString == identifier else { throw Failure.incomplete }

    // Pin the real OS temp directory, then open its one SDK child without following links.
    // Descriptor-relative unlink cannot be redirected by a later namespace rename/symlink swap.
    let rootDescriptor = canonicalRoot.path.withCString { Darwin.open($0, O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW) }
    guard rootDescriptor >= 0 else {
      if errno == ENOENT { return }
      throw Failure.incomplete
    }
    defer { _ = Darwin.close(rootDescriptor) }
    let directoryDescriptor = mediaLibraryExportDirectoryName.withCString { Darwin.openat(rootDescriptor, $0, O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW) }
    guard directoryDescriptor >= 0 else {
      if errno == ENOENT { return }
      throw Failure.incomplete
    }
    defer { _ = Darwin.close(directoryDescriptor) }
    var attributes = stat()
    let inspected = filename.withCString { Darwin.fstatat(directoryDescriptor, $0, &attributes, AT_SYMLINK_NOFOLLOW) }
    guard inspected == 0 else {
      if errno == ENOENT { return }
      throw Failure.incomplete
    }
    guard (attributes.st_mode & mode_t(S_IFMT)) == mode_t(S_IFREG) else { throw Failure.incomplete }
    let removed = filename.withCString { Darwin.unlinkat(directoryDescriptor, $0, 0) }
    guard removed == 0 || errno == ENOENT else { throw Failure.incomplete }
  }
  /// SDK58-owned copies only. Assets, fonts, updates, Documents and Photos stay untouched.
  static func erasePreviewCache(cacheRoot: URL = .cachesDirectory, temporaryRoot: URL = FileManager.default.temporaryDirectory) throws {
    let owned = ["ZoenMedia", "ZoenVoiceEditor", "ImagePicker", "ImageManipulator", "DocumentPicker", "ExpoAudio", "zoen-exports"]
    // SDK58 exports current slow-motion compositions into this exact app-temp namespace.
    // Erasure reaches this only after the existing JS SDK-writer drain. Never sweep tmp itself.
    let directories = owned.map { cacheRoot.appending(path: $0, directoryHint: .isDirectory) } + [
      temporaryRoot.appending(path: mediaLibraryExportDirectoryName, directoryHint: .isDirectory)
    ]
    var failed = false
    for directory in directories {
      do {
        if try exists(directory) { try FileManager.default.removeItem(at: directory) }
        if try exists(directory) { failed = true }
      } catch { failed = true }
    }
    if failed { throw Failure.incomplete }
  }
}
