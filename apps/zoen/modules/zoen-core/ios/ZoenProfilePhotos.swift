import Foundation
import ImageIO
import UniformTypeIdentifiers

/// Disposable local views only. The existing Rust profile remains authoritative.
final class ZoenProfilePhotos {
  static let maximumBytes = 5 * 1024 * 1024
  private let files = ZoenMediaFiles()
  private var owned = Set<String>()

  struct Save: Decodable {
    let ownerId: String
    let deviceId: String
    let name: String
    let bio: String
    let photo: String
    let uri: String?
  }

  enum Failure: Error, LocalizedError {
    case invalidPhoto, changedSession
    var errorDescription: String? {
      switch self {
      case .invalidPhoto: NSLocalizedString("Select a JPEG photo up to 5 MB again.", comment: "Native profile photo is invalid")
      case .changedSession: NSLocalizedString("The session changed. Open your profile again.", comment: "Profile photo session changed")
      }
    }
  }

  func change(_ save: Save) throws -> PhotoChange {
    switch save.photo {
    case "keep": guard save.uri == nil else { throw Failure.invalidPhoto }; return .keep
    case "remove": guard save.uri == nil else { throw Failure.invalidPhoto }; return .remove
    case "set":
      guard let uri = save.uri, let url = URL(string: uri), url.isFileURL else { throw Failure.invalidPhoto }
      let resolved = url.resolvingSymlinksInPath().standardizedFileURL
      let roots = [URL.cachesDirectory, URL.temporaryDirectory].map { $0.resolvingSymlinksInPath().standardizedFileURL.path + "/" }
      guard roots.contains(where: { resolved.path.hasPrefix($0) }) else { throw Failure.invalidPhoto }
      let values = try resolved.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
      guard values.isRegularFile == true, let size = values.fileSize, size > 0, size <= Self.maximumBytes else { throw Failure.invalidPhoto }
      let handle = try FileHandle(forReadingFrom: resolved)
      defer { try? handle.close() }
      let bytes = try handle.read(upToCount: Self.maximumBytes + 1) ?? Data()
      guard bytes.count == size, bytes.count <= Self.maximumBytes,
            let source = CGImageSourceCreateWithData(bytes as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
            CGImageSourceGetType(source) == UTType.jpeg.identifier as CFString,
            CGImageSourceGetCount(source) == 1 else { throw Failure.invalidPhoto }
      return .set(bytes: bytes, mime: "image/jpeg")
    default: throw Failure.invalidPhoto
    }
  }

  func stage(_ photo: ProfilePhotoBytes) throws -> String {
    guard !photo.bytes.isEmpty, photo.bytes.count <= Self.maximumBytes else { throw Failure.invalidPhoto }
    let ext: String
    switch photo.mime { case "image/jpeg": ext = "jpg"; case "image/png": ext = "png"; case "image/heic": ext = "heic"; case "image/webp": ext = "webp"; default: throw Failure.invalidPhoto }
    let uri = try files.stage(photo.bytes, name: "profile.\(ext)")
    owned.insert(uri)
    return try ZoenMediaFiles.json(["uri": uri, "identityId": photo.identityId, "sha256": photo.sha256, "mime": photo.mime])
  }

  func release(_ uri: String) { guard owned.remove(uri) != nil else { return }; files.release(uri) }
  func removeAll() { Array(owned).forEach(release) }
}
