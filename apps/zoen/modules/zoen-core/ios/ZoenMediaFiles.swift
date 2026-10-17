import Foundation
import ImageIO
import UniformTypeIdentifiers

/// Only this object owns decrypted preview copies. Rust owns durable chunks and
/// their encryption keys; JS receives neither bytes nor paths to the database.
final class ZoenMediaFiles {
  // The engine accepts 512 MB; UniFFI currently copies contiguous Data. Limit
  // this mobile bridge to 64 MB until streamed ingestion is available.
  static let maximumBytes: UInt64 = 64 << 20
  private var owned: [String: URL] = [:]

  init() {
    // A killed process cannot release its preview leases. This directory only
    // contains disposable decrypted views, never the Rust store or recordings.
    try? FileManager.default.removeItem(at: URL.cachesDirectory.appending(path: "ZoenMedia", directoryHint: .isDirectory))
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
      case .invalidFile: "Não foi possível ler este arquivo. Selecione-o novamente."
      case .tooLarge: "Escolha um arquivo de até 64 MB."
      case .invalidRecording: "A gravação está vazia ou inválida. Grave novamente."
      case .invalidVersion: "Esta versão do arquivo não está disponível."
      }
    }
  }

  func readSelection(uri: String, name: String, mime: String) throws -> Selection {
    guard let url = URL(string: uri), url.isFileURL,
          !name.isEmpty, name.utf8.count <= 255,
          !name.contains("/"), !name.contains("\\"), !name.contains("\0"),
          mime.utf8.count <= 128, mime.contains("/"), !mime.contains("\0") else { throw Failure.invalidFile }
    let resolved = url.resolvingSymlinksInPath().standardizedFileURL
    let caches = URL.cachesDirectory.resolvingSymlinksInPath().standardizedFileURL.path + "/"
    let temporary = URL.temporaryDirectory.resolvingSymlinksInPath().standardizedFileURL.path + "/"
    // Expo pickers and recordings stage into cache/tmp. A JS-supplied URI must
    // never turn arbitrary app support files into exported attachments.
    guard resolved.path.hasPrefix(caches) || resolved.path.hasPrefix(temporary) else { throw Failure.invalidFile }
    let values = try resolved.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
    guard values.isRegularFile == true, let size = values.fileSize, size > 0 else { throw Failure.invalidFile }
    guard UInt64(size) <= Self.maximumBytes else { throw Failure.tooLarge }
    let data = try Data(contentsOf: resolved, options: [.mappedIfSafe])
    guard !data.isEmpty, data.count == size, UInt64(data.count) <= Self.maximumBytes else { throw Failure.tooLarge }
    let detected = UTType(filenameExtension: resolved.pathExtension)?.preferredMIMEType
    return Selection(url: resolved, name: name, mime: mime == "application/octet-stream" ? (detected ?? mime) : mime, data: data)
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
    let root = URL.cachesDirectory.appending(path: "ZoenMedia/\(UUID().uuidString)", directoryHint: .isDirectory)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true, attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
    let safe = name.split(whereSeparator: { $0 == "/" || $0 == "\\" || $0 == "\0" }).last.map(String.init) ?? "arquivo"
    let file = root.appendingPathComponent(safe == "." || safe == ".." ? "arquivo" : safe)
    do { try bytes.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication]) }
    catch { try? FileManager.default.removeItem(at: root); throw error }
    owned[file.absoluteString] = root
    return file.absoluteString
  }

  func release(_ uri: String) {
    guard let root = owned.removeValue(forKey: uri) else { return }
    try? FileManager.default.removeItem(at: root)
  }

  func removeAll() { Array(owned.keys).forEach(release) }

  static func info(_ file: FileDto) -> [String: Any] {
    ["name": file.name, "path": file.path, "mime": file.mime, "bytes": String(file.bytes),
     "sha256": file.sha256, "ready": file.ready, "chunks": file.chunks,
     "chunks_here": file.chunksHere, "has_thumbnail": file.hasThumbnail]
  }

  static func json(_ value: [String: Any]) throws -> String {
    String(decoding: try JSONSerialization.data(withJSONObject: value), as: UTF8.self)
  }
}
