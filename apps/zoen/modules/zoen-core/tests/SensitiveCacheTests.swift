import Foundation

@main struct SensitiveCacheTests {
  static func main() throws {
    let manager = FileManager.default
    let sandbox = manager.temporaryDirectory.appendingPathComponent("zoen-sensitive-cache-fixture-\(UUID().uuidString)", isDirectory: true)
    defer { try? manager.removeItem(at: sandbox) }
    let cache = sandbox.appendingPathComponent("Library/Caches", isDirectory: true)
    let owned = ["ZoenMedia", "ZoenVoiceEditor", "ImagePicker", "ImageManipulator", "DocumentPicker", "ExpoAudio", "zoen-exports"]
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
    // An inaccessible parent is an inspection failure, never an empty cache.
    try manager.createDirectory(at: blocked, withIntermediateDirectories: true)
    let retained = blocked.appendingPathComponent("unreadable-parent.jpg")
    try Data("synthetic inaccessible copy".utf8).write(to: retained)
    try manager.setAttributes([.posixPermissions: 0o000], ofItemAtPath: cache.path)
    defer { try? manager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: cache.path) }
    do {
      try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache)
      fatalError("unreadable cache parent accepted as empty")
    } catch ZoenDeviceErasure.Failure.incomplete {}
    try manager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: cache.path)
    precondition(manager.fileExists(atPath: retained.path))
    try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache)
    precondition(!manager.fileExists(atPath: retained.path))
    // `attributesOfItem` observes the link itself, including a dangling cache link.
    let dangling = cache.appendingPathComponent("ImagePicker")
    try manager.createSymbolicLink(at: dangling, withDestinationURL: sandbox.appendingPathComponent("missing-original"))
    try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache)
    do { _ = try manager.attributesOfItem(atPath: dangling.path); fatalError("dangling cache link retained") }
    catch let error as CocoaError where error.code == .fileNoSuchFile || error.code == .fileReadNoSuchFile {}
    print("Synthetic cache sweep passed: seven namespaces including voice originals/exports, external originals/assets preserved, symlinks, denied child/root inspection and retry.")
  }
}
