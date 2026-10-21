import Foundation

@main struct SensitiveCacheTests {
  static func main() throws {
    let manager = FileManager.default
    let sandbox = manager.temporaryDirectory.appendingPathComponent("zoen-sensitive-cache-fixture-\(UUID().uuidString)", isDirectory: true)
    defer { try? manager.removeItem(at: sandbox) }
    let cache = sandbox.appendingPathComponent("Library/Caches", isDirectory: true)
    let temporary = sandbox.appendingPathComponent("tmp", isDirectory: true)
    let mediaLibrary = temporary.appendingPathComponent("MediaLibraryUriExtractorDirectory", isDirectory: true)
    let unrelatedTemporary = ["MediaLibraryUriExtractorDirectory-other", "ImagePicker", "unrelated"]
    for name in unrelatedTemporary {
      let directory = temporary.appendingPathComponent(name, isDirectory: true)
      try manager.createDirectory(at: directory, withIntermediateDirectories: true)
      try Data("synthetic unrelated temporary file".utf8).write(to: directory.appendingPathComponent("keep"))
    }
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
    try manager.createDirectory(at: mediaLibrary, withIntermediateDirectories: true)
    try Data("synthetic rendered current slow-motion export".utf8).write(to: mediaLibrary.appendingPathComponent("slowMoVideo-fixture.mov"))
    // SDK failure/crash can leave an export without ever returning its URI to JavaScript.
    let interruptedExport = mediaLibrary.appendingPathComponent("slowMoVideo-AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE.mov")
    try Data("synthetic partial export without returned URI".utf8).write(to: interruptedExport)
    // A link inside the owned namespace must never remove a library original.
    try manager.createSymbolicLink(at: mediaLibrary.appendingPathComponent("external-original"), withDestinationURL: photos)
    try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache, temporaryRoot: temporary)
    precondition(!manager.fileExists(atPath: mediaLibrary.path))
    for name in unrelatedTemporary { precondition(manager.fileExists(atPath: temporary.appendingPathComponent(name).appendingPathComponent("keep").path), name) }
    for name in owned { precondition(!manager.fileExists(atPath: cache.appendingPathComponent(name).path), name) }
    for name in preserved { precondition(manager.fileExists(atPath: cache.appendingPathComponent(name).appendingPathComponent("fixture").path), name) }
    for directory in [photos, documents] { precondition(manager.fileExists(atPath: directory.appendingPathComponent("original.jpg").path)) }
    // Removing an owned cache symlink must remove the link, never follow it to an original.
    try manager.createSymbolicLink(at: cache.appendingPathComponent("ImagePicker"), withDestinationURL: photos)
    try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache, temporaryRoot: temporary)
    precondition(manager.fileExists(atPath: photos.appendingPathComponent("original.jpg").path))
    try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache, temporaryRoot: temporary)
    // A denied namespace must keep erasure retryable and not skip other namespaces.
    let blocked = cache.appendingPathComponent("ImagePicker", isDirectory: true)
    let independent = cache.appendingPathComponent("ExpoAudio", isDirectory: true)
    for directory in [blocked, independent] {
      try manager.createDirectory(at: directory, withIntermediateDirectories: true)
      try Data("synthetic blocked copy".utf8).write(to: directory.appendingPathComponent("fixture"))
    }
    try manager.createDirectory(at: mediaLibrary, withIntermediateDirectories: true)
    try Data("synthetic interrupted export remains independently sweepable".utf8).write(to: interruptedExport)
    try manager.setAttributes([.posixPermissions: 0o000], ofItemAtPath: blocked.path)
    defer { try? manager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: blocked.path) }
    do {
      try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache, temporaryRoot: temporary)
      fatalError("denied cache removal accepted")
    } catch ZoenDeviceErasure.Failure.incomplete {}
    precondition(!manager.fileExists(atPath: independent.path))
    precondition(!manager.fileExists(atPath: mediaLibrary.path))
    try manager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: blocked.path)
    try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache, temporaryRoot: temporary)
    precondition(!manager.fileExists(atPath: blocked.path))
    // An inaccessible parent is an inspection failure, never an empty cache.
    try manager.createDirectory(at: blocked, withIntermediateDirectories: true)
    let retained = blocked.appendingPathComponent("unreadable-parent.jpg")
    try Data("synthetic inaccessible copy".utf8).write(to: retained)
    try manager.setAttributes([.posixPermissions: 0o000], ofItemAtPath: cache.path)
    defer { try? manager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: cache.path) }
    do {
      try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache, temporaryRoot: temporary)
      fatalError("unreadable cache parent accepted as empty")
    } catch ZoenDeviceErasure.Failure.incomplete {}
    try manager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: cache.path)
    precondition(manager.fileExists(atPath: retained.path))
    try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache, temporaryRoot: temporary)
    precondition(!manager.fileExists(atPath: retained.path))
    // `attributesOfItem` observes the link itself, including a dangling cache link.
    let dangling = cache.appendingPathComponent("ImagePicker")
    try manager.createSymbolicLink(at: dangling, withDestinationURL: sandbox.appendingPathComponent("missing-original"))
    try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache, temporaryRoot: temporary)
    do { _ = try manager.attributesOfItem(atPath: dangling.path); fatalError("dangling cache link retained") }
    catch let error as CocoaError where error.code == .fileNoSuchFile || error.code == .fileReadNoSuchFile {}
    // The exact temp namespace is removed as a link, never followed to Photos/Documents.
    for destination in [photos, documents, sandbox.appendingPathComponent("missing-temp-original")] {
      try manager.createSymbolicLink(at: mediaLibrary, withDestinationURL: destination)
      try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache, temporaryRoot: temporary)
      do { _ = try manager.attributesOfItem(atPath: mediaLibrary.path); fatalError("SDK temp namespace link retained") }
      catch let error as CocoaError where error.code == .fileNoSuchFile || error.code == .fileReadNoSuchFile {}
      for directory in [photos, documents] { precondition(manager.fileExists(atPath: directory.appendingPathComponent("original.jpg").path)) }
    }
    // A denied SDK export directory keeps erasure retryable; old cache namespaces still drain.
    try manager.createDirectory(at: mediaLibrary, withIntermediateDirectories: true)
    let currentExport = mediaLibrary.appendingPathComponent("slowMoVideo-denied.mov")
    try Data("synthetic denied current export".utf8).write(to: currentExport)
    try manager.createDirectory(at: independent, withIntermediateDirectories: true)
    try Data("synthetic independent copy".utf8).write(to: independent.appendingPathComponent("fixture"))
    try manager.setAttributes([.posixPermissions: 0o000], ofItemAtPath: mediaLibrary.path)
    defer { try? manager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: mediaLibrary.path) }
    do {
      try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache, temporaryRoot: temporary)
      fatalError("denied SDK temp removal accepted")
    } catch ZoenDeviceErasure.Failure.incomplete {}
    precondition(!manager.fileExists(atPath: independent.path))
    try manager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: mediaLibrary.path)
    precondition(manager.fileExists(atPath: currentExport.path))
    try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache, temporaryRoot: temporary)
    precondition(!manager.fileExists(atPath: mediaLibrary.path))
    // Parent inspection failure is not an empty SDK namespace and cannot silently succeed.
    try manager.createDirectory(at: mediaLibrary, withIntermediateDirectories: true)
    try Data("synthetic parent-denied export".utf8).write(to: currentExport)
    try manager.setAttributes([.posixPermissions: 0o000], ofItemAtPath: temporary.path)
    defer { try? manager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: temporary.path) }
    do {
      try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache, temporaryRoot: temporary)
      fatalError("unreadable SDK temp parent accepted as empty")
    } catch ZoenDeviceErasure.Failure.incomplete {}
    try manager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: temporary.path)
    precondition(manager.fileExists(atPath: currentExport.path))
    try ZoenDeviceErasure.erasePreviewCache(cacheRoot: cache, temporaryRoot: temporary)
    for name in unrelatedTemporary { precondition(manager.fileExists(atPath: temporary.appendingPathComponent(name).appendingPathComponent("keep").path), name) }
    try verifyMediaLibraryExportRelease(sandbox: sandbox)
    print("Synthetic cache sweep and exact SDK export release passed: known namespaces only, originals/unrelated tmp preserved, canonical UUID paths, symlinks, denied inspection and retry.")
  }

  private static func verifyMediaLibraryExportRelease(sandbox: URL) throws {
    let manager = FileManager.default
    let temporary = sandbox.appendingPathComponent("release-fixture/tmp", isDirectory: true)
    let namespace = temporary.appendingPathComponent("MediaLibraryUriExtractorDirectory", isDirectory: true)
    let identifier = "01234567-89AB-CDEF-0123-456789ABCDEF"
    let filename = "slowMoVideo-\(identifier).mov"
    let current = namespace.appendingPathComponent(filename)
    let other = namespace.appendingPathComponent("slowMoVideo-FEDCBA98-7654-3210-FEDC-BA9876543210.mov")
    let original = sandbox.appendingPathComponent("Photos/\(filename)")
    try manager.createDirectory(at: namespace, withIntermediateDirectories: true)
    for file in [current, other, original] { try Data("synthetic SDK export or original".utf8).write(to: file) }
    try ZoenDeviceErasure.releaseMediaLibraryExport(original.absoluteString, temporaryRoot: temporary)
    precondition(manager.fileExists(atPath: original.path))
    try ZoenDeviceErasure.releaseMediaLibraryExport(current.absoluteString, temporaryRoot: temporary)
    precondition(!manager.fileExists(atPath: current.path))
    precondition(manager.fileExists(atPath: other.path))
    try ZoenDeviceErasure.releaseMediaLibraryExport(current.absoluteString, temporaryRoot: temporary)
    try Data("synthetic canonical export".utf8).write(to: current)
    let encoded = current.absoluteString.replacingOccurrences(of: "slowMoVideo-", with: "slowMoVideo%2D")
    let traversal = namespace.absoluteString + "../MediaLibraryUriExtractorDirectory/" + filename
    let nested = namespace.appendingPathComponent("nested/" + filename).absoluteString
    for uri in [encoded, traversal, nested, current.absoluteString + "?copy=1", current.absoluteString + "#copy", namespace.appendingPathComponent("slowMoVideo-invalid.mov").absoluteString] {
      do { try ZoenDeviceErasure.releaseMediaLibraryExport(uri, temporaryRoot: temporary); fatalError("noncanonical SDK export accepted") }
      catch ZoenDeviceErasure.Failure.incomplete {}
      precondition(manager.fileExists(atPath: current.path))
      precondition(manager.fileExists(atPath: original.path))
    }
    // Final-file symlinks are rejected and their external targets stay intact.
    try manager.removeItem(at: current)
    try manager.createSymbolicLink(at: current, withDestinationURL: original)
    do { try ZoenDeviceErasure.releaseMediaLibraryExport(current.absoluteString, temporaryRoot: temporary); fatalError("SDK export symlink accepted") }
    catch ZoenDeviceErasure.Failure.incomplete {}
    precondition(manager.fileExists(atPath: original.path))
    let linkAttributes = try manager.attributesOfItem(atPath: current.path)
    precondition((linkAttributes[.type] as? FileAttributeType) == .typeSymbolicLink)
    try manager.removeItem(at: current)
    try manager.createDirectory(at: current, withIntermediateDirectories: true)
    do { try ZoenDeviceErasure.releaseMediaLibraryExport(current.absoluteString, temporaryRoot: temporary); fatalError("SDK export directory accepted") }
    catch ZoenDeviceErasure.Failure.incomplete {}
    try manager.removeItem(at: current)
    // A namespace link must not redirect unlink into an original directory.
    try manager.removeItem(at: namespace)
    try manager.createSymbolicLink(at: namespace, withDestinationURL: original.deletingLastPathComponent())
    do { try ZoenDeviceErasure.releaseMediaLibraryExport(current.absoluteString, temporaryRoot: temporary); fatalError("SDK namespace symlink accepted") }
    catch ZoenDeviceErasure.Failure.incomplete {}
    precondition(manager.fileExists(atPath: original.path))
    try manager.removeItem(at: namespace)
    try manager.createDirectory(at: namespace, withIntermediateDirectories: true)
    try Data("synthetic retryable export".utf8).write(to: current)
    try manager.setAttributes([.posixPermissions: 0o000], ofItemAtPath: namespace.path)
    defer { try? manager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: namespace.path) }
    do { try ZoenDeviceErasure.releaseMediaLibraryExport(current.absoluteString, temporaryRoot: temporary); fatalError("denied SDK export accepted") }
    catch ZoenDeviceErasure.Failure.incomplete {}
    try manager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: namespace.path)
    precondition(manager.fileExists(atPath: current.path))
    try ZoenDeviceErasure.releaseMediaLibraryExport(current.absoluteString, temporaryRoot: temporary)
    precondition(!manager.fileExists(atPath: current.path))
    precondition(manager.fileExists(atPath: original.path))
  }
}
