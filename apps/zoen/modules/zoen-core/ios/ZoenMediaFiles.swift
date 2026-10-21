import Foundation
import ImageIO
import UniformTypeIdentifiers

/// Only this object owns decrypted preview copies. Rust owns durable chunks and
/// their encryption keys; JS receives neither bytes nor paths to the database.
final class ZoenMediaFiles {
  // The engine accepts 512 MB; UniFFI currently copies contiguous Data. Limit
  // this mobile bridge to 64 MB until streamed ingestion is available.
  static let maximumBytes: UInt64 = 64 << 20
  private let cacheRoot: URL
  private let temporaryRoot: URL
  // Canonical Selection.url paths, never caller-supplied URI spellings.
  private var owned: [String: URL] = [:]

  init(cacheRoot: URL = .cachesDirectory, temporaryRoot: URL = .temporaryDirectory) {
    self.cacheRoot = cacheRoot
    self.temporaryRoot = temporaryRoot
    // A killed process cannot release its preview leases. This directory only
    // contains disposable decrypted views, never the Rust store or recordings.
    try? FileManager.default.removeItem(at: cacheRoot.appending(path: "ZoenMedia", directoryHint: .isDirectory))
  }

  struct Selection {
    let url: URL
    let name: String
    let mime: String
    let data: Data
  }

  enum Failure: Error, LocalizedError {
    case invalidFile, tooLarge, invalidRecording, invalidVersion
    var errorDescription: String? {
      switch self {
      case .invalidFile: NSLocalizedString("This file could not be read. Select it again.", comment: "Native media file selection failed")
      case .tooLarge: NSLocalizedString("Choose a file up to 64 MB.", comment: "Native media bridge file size limit")
      case .invalidRecording: NSLocalizedString("The recording is empty or invalid. Record it again.", comment: "Native voice recording is unavailable")
      case .invalidVersion: NSLocalizedString("This file version is unavailable.", comment: "Native media version is unavailable")
      }
    }
  }

  func readSelection(uri: String, name: String, mime: String) throws -> Selection {
    try readSelection(uri: uri, name: name, mime: mime, captured: nil)
  }

  /// Only the existing native preview/capture owner supplies this typed lease.
  /// It is never an alternative admission flag on the JS URI selection API.
  func readCapturedSelection(_ file: ZoenPreviewFile, name: String, mime: String) throws -> Selection {
    let validated = try ZoenPreviewFile.validated(url: file.url, directory: file.directory,
      cacheRoot: cacheRoot, maximumBytes: Self.maximumBytes)
    return try readSelection(uri: validated.url.absoluteString, name: name, mime: mime, captured: validated)
  }

  private func readSelection(uri: String, name: String, mime: String, captured: ZoenPreviewFile?) throws -> Selection {
    guard !name.isEmpty, name.utf8.count <= 255,
          !name.contains("/"), !name.contains("\\"), !name.contains("\0"),
          mime.utf8.count <= 128, mime.contains("/"), !mime.contains("\0") else { throw Failure.invalidFile }
    let resolved = try Self.selectionURL(uri)
    let caches = cacheRoot.resolvingSymlinksInPath().standardizedFileURL.path + "/"
    let temporary = temporaryRoot.resolvingSymlinksInPath().standardizedFileURL.path + "/"
    // Expo pickers and recordings stage into cache/tmp. A JS-supplied URI must
    // never turn arbitrary app support files into exported attachments.
    guard resolved.path.hasPrefix(caches) || resolved.path.hasPrefix(temporary) else { throw Failure.invalidFile }
    // Transferred/released preview bytes must never re-enter through the
    // ordinary picker path, even if deletion failed or an alias changes case.
    // Case spellings not normalized by the filesystem are conservatively denied.
    if Self.isManagedNamespace(URL(string: uri)!) || Self.isManagedNamespace(resolved) {
      if let captured {
        guard resolved == captured.url else { throw Failure.invalidFile }
      } else { _ = try ownedSelectionURL(uri) }
    }
    let values = try resolved.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
    guard values.isRegularFile == true, let size = values.fileSize, size > 0 else { throw Failure.invalidFile }
    guard UInt64(size) <= Self.maximumBytes else { throw Failure.tooLarge }
    let data = try Data(contentsOf: resolved, options: [.mappedIfSafe])
    guard !data.isEmpty, data.count == size, UInt64(data.count) <= Self.maximumBytes else { throw Failure.tooLarge }
    let detected = UTType(filenameExtension: resolved.pathExtension)?.preferredMIMEType
    return Selection(url: resolved, name: name, mime: mime == "application/octet-stream" ? (detected ?? mime) : mime, data: data)
  }

  static func selectionURL(_ uri: String) throws -> URL {
    guard let url = URL(string: uri), url.isFileURL else { throw Failure.invalidFile }
    // Reconstruct from the resolved filesystem path so percent escapes, query
    // and fragment spellings cannot become a different provenance dictionary key.
    return URL(fileURLWithPath: url.resolvingSymlinksInPath().standardizedFileURL.path).standardizedFileURL
  }

  private static func isManagedNamespace(_ url: URL) -> Bool {
    let parts = url.standardizedFileURL.pathComponents
    // Inspect the lexical spelling too: a substituted file/directory symlink
    // may resolve out of the namespace. Cache ancestor aliases stay accepted.
    return parts.indices.contains { index in
      index + 2 < parts.count && parts[index].caseInsensitiveCompare("ZoenMedia") == .orderedSame && UUID(uuidString: parts[index + 1]) != nil
    }
  }

  func ownedSelectionURL(_ uri: String) throws -> URL {
    let url = try Self.selectionURL(uri)
    guard let directory = owned[url.path] else { throw Failure.invalidFile }
    return try ZoenPreviewFile.validated(url: url, directory: directory,
      cacheRoot: cacheRoot, maximumBytes: Self.maximumBytes).url
  }

  func releaseIdentity(_ uri: String) -> String? {
    if let path = try? Self.selectionURL(uri).path, owned[path] != nil { return path }
    guard let url = URL(string: uri), url.isFileURL else { return nil }
    let path = url.standardizedFileURL.path
    let canonicalRoot = cacheRoot.resolvingSymlinksInPath().standardizedFileURL.path
    // The returned canonical stage URI still identifies our lease if its file
    // or UUID directory was substituted. Never follow that substitution to
    // delete another cache file; only the pre-existing owned key can be retired.
    for root in [cacheRoot.standardizedFileURL.path, canonicalRoot] where path.hasPrefix(root + "/") {
      let key = canonicalRoot + path.dropFirst(root.count)
      if owned[key] != nil { return key }
    }
    return nil
  }

  static func version(_ value: Int?) throws -> UInt32? {
    guard let value else { return nil }
    guard value > 0, let result = UInt32(exactly: value) else { throw Failure.invalidVersion }
    return result
  }

  static func thumbnail(at url: URL) throws -> Data? {
    guard let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary),
          let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: 480,
            kCGImageSourceShouldCacheImmediately: false,
          ] as CFDictionary) else { return nil }
    let data = NSMutableData()
    guard let output = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil) else { return nil }
    CGImageDestinationAddImage(output, image, nil)
    guard CGImageDestinationFinalize(output), data.length <= 2 << 20 else { return nil }
    return data as Data
  }

  func stage(_ bytes: Data, name: String) throws -> String {
    guard !bytes.isEmpty, UInt64(bytes.count) <= Self.maximumBytes else { throw Failure.tooLarge }
    let root = cacheRoot.appending(path: "ZoenMedia/\(UUID().uuidString)", directoryHint: .isDirectory)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true, attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
    let safe = name.split(whereSeparator: { $0 == "/" || $0 == "\\" || $0 == "\0" }).last.map(String.init) ?? "arquivo"
    let file = root.appendingPathComponent(safe == "." || safe == ".." ? "arquivo" : safe)
    do { try bytes.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication]) }
    catch { try? FileManager.default.removeItem(at: root); throw error }
    owned[try Self.selectionURL(file.absoluteString).path] = root
    return file.absoluteString
  }

  func release(_ uri: String) {
    guard let identity = releaseIdentity(uri), let root = owned.removeValue(forKey: identity) else { return }
    try? FileManager.default.removeItem(at: root)
  }

  func takeForPreview(_ uri: String) throws -> ZoenPreviewFile {
    let url = try ownedSelectionURL(uri)
    guard let directory = owned[url.path] else { throw Failure.invalidFile }
    let file = try ZoenPreviewFile.validated(url: url, directory: directory,
      cacheRoot: cacheRoot, maximumBytes: Self.maximumBytes)
    owned.removeValue(forKey: url.path)
    return file
  }

  func removeAll() { Array(owned.keys).forEach { release(URL(fileURLWithPath: $0).absoluteString) } }

  static func info(_ file: FileDto) -> [String: Any] {
    ["name": file.name, "path": file.path, "mime": file.mime, "bytes": String(file.bytes),
     "sha256": file.sha256, "ready": file.ready, "chunks": file.chunks,
     "chunks_here": file.chunksHere, "has_thumbnail": file.hasThumbnail]
  }

  static func json(_ value: [String: Any]) throws -> String {
    String(decoding: try JSONSerialization.data(withJSONObject: value), as: UTF8.self)
  }
}
