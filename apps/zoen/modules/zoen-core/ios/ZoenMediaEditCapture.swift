import Foundation
import ImageIO
import AVFoundation
import UniformTypeIdentifiers

/// A synchronous owned copy outlives Quick Look's temporary delegate URL.
/// The original draft is untouched; only an explicitly saved copy can reach CAS.
enum ZoenMediaEditCapture {
  enum Kind: Sendable { case image, video }
  struct Metadata: Sendable {
    let mime: String
    let fileExtension: String
  }
  static func kind(mime: String) -> Kind? {
    switch mime {
    case "image/jpeg", "image/png", "image/heic", "image/heif": .image
    case "video/mp4", "video/quicktime", "video/x-m4v": .video
    default: nil
    }
  }
  /// The in-app editor renders into memory. Write directly into the same
  /// tracked preview owner; there is no additional disposable plaintext URL.
  static func stageImage(_ png: Data, maximumBytes: UInt64) -> Result<ZoenPreviewFile, ZoenPDFEditCapture.Failure> {
    let manager = FileManager.default
    var copy: ZoenPreviewFile?
    do {
      guard !png.isEmpty, UInt64(png.count) <= maximumBytes,
            let source = CGImageSourceCreateWithData(png as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
            CGImageSourceGetType(source) as String? == UTType.png.identifier,
            CGImageSourceGetCount(source) == 1,
            CGImageSourceGetStatus(source) == .statusComplete,
            CGImageSourceGetStatusAtIndex(source, 0) == .statusComplete else {
        throw ZoenFilePreview.Failure.editedMedia
      }
      let directory = URL.cachesDirectory.appendingPathComponent("ZoenMedia/\(UUID().uuidString)", isDirectory: true)
      try manager.createDirectory(at: directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
      let file = ZoenPreviewFile(url: directory.appendingPathComponent("edited.png"), directory: directory)
      copy = file
      #if os(iOS)
      try manager.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: directory.path)
      #endif
      try png.write(to: file.url, options: .withoutOverwriting)
      try manager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: file.url.path)
      #if os(iOS)
      try manager.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: file.url.path)
      #endif
      try regular(file.url, maximumBytes: maximumBytes)
      guard try metadata(file.url, kind: .image).mime == "image/png" else { throw ZoenFilePreview.Failure.editedMedia }
      return .success(file)
    } catch {
      if let copy {
        do { try copy.remove() }
        catch { return .failure(.init(cause: error, retained: copy)) }
      }
      return .failure(.init(cause: error, retained: nil))
    }
  }
  static func stage(_ source: URL, kind: Kind, maximumBytes: UInt64) -> Result<ZoenPreviewFile, ZoenPDFEditCapture.Failure> {
    let manager = FileManager.default
    var copy: ZoenPreviewFile?
    let scoped = source.startAccessingSecurityScopedResource()
    defer { if scoped { source.stopAccessingSecurityScopedResource() } }
    do {
      try regular(source, maximumBytes: maximumBytes)
      let metadata = try metadata(source, kind: kind)
      let directory = URL.cachesDirectory.appendingPathComponent("ZoenMedia/\(UUID().uuidString)", isDirectory: true)
      try manager.createDirectory(at: directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
      let file = ZoenPreviewFile(url: directory.appendingPathComponent("edited.\(metadata.fileExtension)"), directory: directory)
      copy = file
      try manager.copyItem(at: source, to: file.url)
      try manager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: file.url.path)
      #if os(iOS)
      try manager.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: directory.path)
      try manager.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: file.url.path)
      #endif
      try regular(file.url, maximumBytes: maximumBytes)
      _ = try self.metadata(file.url, kind: kind)
      return .success(file)
    } catch {
      if let copy {
        do { try copy.remove() }
        catch { return .failure(.init(cause: error, retained: copy)) }
      }
      return .failure(.init(cause: error, retained: nil))
    }
  }
  static func validate(_ file: ZoenPreviewFile, kind: Kind, maximumBytes: UInt64) async throws -> Metadata {
    try regular(file.url, maximumBytes: maximumBytes)
    let info = try metadata(file.url, kind: kind)
    if kind == .video {
      let asset = AVURLAsset(url: file.url)
      let duration = try await asset.load(.duration)
      let tracks = try await asset.loadTracks(withMediaType: .video)
      guard duration.seconds.isFinite, duration.seconds > 0, !tracks.isEmpty else {
        throw ZoenFilePreview.Failure.editedMedia
      }
    }
    return info
  }
  private static func regular(_ url: URL, maximumBytes: UInt64) throws {
    guard url.isFileURL else { throw ZoenFilePreview.Failure.editedMedia }
    let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
    guard attributes[.type] as? FileAttributeType == .typeRegular,
          let size = attributes[.size] as? NSNumber,
          size.uint64Value > 0, size.uint64Value <= maximumBytes else {
      throw ZoenFilePreview.Failure.editedMedia
    }
  }
  private static func metadata(_ url: URL, kind: Kind) throws -> Metadata {
    if kind == .image {
      guard let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary),
            CGImageSourceGetCount(source) == 1, CGImageSourceGetStatus(source) == .statusComplete,
            CGImageSourceGetStatusAtIndex(source, 0) == .statusComplete, let identifier = CGImageSourceGetType(source),
            let type = UTType(identifier as String),
            let mime = type.preferredMIMEType,
            self.kind(mime: mime) == .image,
            let ext = type.preferredFilenameExtension,
            let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
            let width = properties[kCGImagePropertyPixelWidth] as? NSNumber,
            let height = properties[kCGImagePropertyPixelHeight] as? NSNumber,
            width.doubleValue > 0, height.doubleValue > 0 else {
        throw ZoenFilePreview.Failure.editedMedia
      }
      return Metadata(mime: mime, fileExtension: ext)
    }
    // Full track/duration validation follows asynchronously after the owned copy exists.
    guard let type = UTType(filenameExtension: url.pathExtension),
          let mime = type.preferredMIMEType, self.kind(mime: mime) == .video,
          let ext = type.preferredFilenameExtension else { throw ZoenFilePreview.Failure.editedMedia }
    return Metadata(mime: mime, fileExtension: ext)
  }
}

/// Cancellation and a synchronous canonical CAS have a defined winning order.
/// This gate stores no account content, URI, bytes, or durable state.
final class ZoenMediaEditGate: @unchecked Sendable {
  private let lock = NSLock()
  private var cancelled = false
  func cancel() { lock.lock(); defer { lock.unlock() }; cancelled = true }
  func admitted<T>(_ work: () throws -> T) throws -> T {
    lock.lock(); defer { lock.unlock() }
    guard !cancelled else { throw CancellationError() }
    return try work()
  }
}

/// A native background transition fences draft edits without waiting for JS.
final class ZoenMediaEditLifetime: @unchecked Sendable {
  private let lock = NSLock()
  private var gates: [UUID: ZoenMediaEditGate] = [:]
  private var active = true
  func activate() { lock.lock(); defer { lock.unlock() }; active = true }
  func register(_ token: UUID, gate: ZoenMediaEditGate) throws {
    lock.lock(); defer { lock.unlock() }
    guard active, gates[token] == nil else { throw CancellationError() }
    gates[token] = gate
  }
  func remove(_ token: UUID) { lock.lock(); defer { lock.unlock() }; gates.removeValue(forKey: token) }
  func cancel(_ token: UUID) { lock.lock(); defer { lock.unlock() }; gates[token]?.cancel() }
  @discardableResult func retire() -> [UUID] {
    lock.lock(); defer { lock.unlock() }
    active = false
    for gate in gates.values { gate.cancel() }
    return Array(gates.keys)
  }
}
