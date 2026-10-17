import Foundation

@main struct SensitiveCacheTests {
  static func main() throws {
    let manager = FileManager.default
    let sandbox = manager.temporaryDirectory.appendingPathComponent("zoen-sensitive-cache-fixture-\(UUID().uuidString)", isDirectory: true)
    defer { try? manager.removeItem(at: sandbox) }
    let cache = sandbox.appendingPathComponent("Library/Caches", isDirectory: true)
    let owned = ["ZoenMedia", "ImagePicker", "ImageManipulator", "DocumentPicker", "ExpoAudio", "zoen-exports"]
    let preserved = ["ExpoAsset", "ExponentAsset", "expo-updates", "fonts", "com.apple.WebKit"]
    for name in owned + preserved {
      let directory = cache.appendingPathComponent(name, isDirectory: true)
      try manager.createDirectory(at: directory, withIntermediateDirectories: true)
      try Data("synthetic fixture".utf8).write(to: directory.appendingPathComponent("fixture"))
    }
    let photos = sandbox.appendingPathComponent("Photos", isDirectory: true)
    let documents = sandbox.appendingPathComponent("Documents", isDirectory: true)
    for directory in [photos, documents] {
      try manager.createDirectory(at: directory, withIntermediateDirectories: true)
      try Data("synthetic original".utf8).write(to: directory.appendingPathComponent("original.jpg"))
    }
    try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache)
    for name in owned { precondition(!manager.fileExists(atPath: cache.appendingPathComponent(name).path), name) }
    for name in preserved { precondition(manager.fileExists(atPath: cache.appendingPathComponent(name).appendingPathComponent("fixture").path), name) }
    for directory in [photos, documents] { precondition(manager.fileExists(atPath: directory.appendingPathComponent("original.jpg").path)) }
    // Removing an owned cache symlink must remove the link, never follow it to an original.
    try manager.createSymbolicLink(at: cache.appendingPathComponent("ImagePicker"), withDestinationURL: photos)
    try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache)
    precondition(manager.fileExists(atPath: photos.appendingPathComponent("original.jpg").path))
    try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache)
    // A denied namespace must keep erasure retryable and not skip other namespaces.
    let blocked = cache.appendingPathComponent("ImagePicker", isDirectory: true)
    let independent = cache.appendingPathComponent("ExpoAudio", isDirectory: true)
    for directory in [blocked, independent] {
      try manager.createDirectory(at: directory, withIntermediateDirectories: true)
      try Data("synthetic blocked copy".utf8).write(to: directory.appendingPathComponent("fixture"))
    }
    try manager.setAttributes([.posixPermissions: 0o000], ofItemAtPath: blocked.path)
    defer { try? manager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: blocked.path) }
    do {
      try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache)
      fatalError("denied cache removal accepted")
    } catch ZoenDeviceErasure.Failure.incomplete {}
    precondition(!manager.fileExists(atPath: independent.path))
    try manager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: blocked.path)
    try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache)
    precondition(!manager.fileExists(atPath: blocked.path))
    print("Synthetic cache sweep passed: six owned namespaces, asset/update/font preservation, external originals, symlink, denied cleanup and retry.")
  }
}
