import Foundation

@MainActor extension FilePreviewTests {
  static func validatedLeaseAcceptsCacheAliasesAndRejectsSubstitutedFiles() throws {
    let fixture = try PreviewFixture()
    defer { fixture.cleanup() }
    let manager = FileManager.default
    let sandbox = fixture.root.resolvingSymlinksInPath().standardizedFileURL
    let cache = sandbox.appendingPathComponent("Library/Caches", isDirectory: true)
    let namespace = cache.appendingPathComponent("ZoenMedia", isDirectory: true)
    let ownedDirectory = namespace.appendingPathComponent(UUID().uuidString, isDirectory: true)
    let siblingDirectory = namespace.appendingPathComponent(UUID().uuidString, isDirectory: true)
    let externalDirectory = sandbox.appendingPathComponent("ArbitraryExternalOriginal", isDirectory: true)
    let externalBytes = Data("Synthetic external original must survive".utf8)

    func write(_ directory: URL, name: String = "synthetic.pdf", bytes: Data = Data("Synthetic validated lease".utf8)) throws -> URL {
      try manager.createDirectory(at: directory, withIntermediateDirectories: true)
      let url = directory.appendingPathComponent(name)
      try bytes.write(to: url)
      return url
    }
    func reject(_ url: URL, directory: URL, maximumBytes: UInt64 = 4096) {
      do {
        _ = try ZoenPreviewFile.validated(url: url, directory: directory, cacheRoot: cache, maximumBytes: maximumBytes)
        preconditionFailure("A substituted or invalid lease was accepted: \(url.lastPathComponent)")
      } catch ZoenFilePreview.Failure.unavailable {
      } catch {
        preconditionFailure("Unexpected lease validation error: \(error)")
      }
    }

    let ownedURL = try write(ownedDirectory)
    let siblingURL = try write(siblingDirectory)
    let externalURL = try write(externalDirectory, bytes: externalBytes)
    let plain = try ZoenPreviewFile.validated(url: ownedURL, directory: ownedDirectory, cacheRoot: cache, maximumBytes: 4096)
    precondition(plain.url == ownedURL)
    precondition(plain.directory == ownedDirectory)

    // Model iOS's legitimate /var -> /private/var ancestor alias with a real
    // filesystem symlink, without permitting a link at the generated leaf.
    let aliasCache = sandbox.appendingPathComponent("OSCacheAncestorAlias", isDirectory: true)
    try manager.createSymbolicLink(at: aliasCache, withDestinationURL: cache)
    let aliasDirectory = aliasCache.appendingPathComponent("ZoenMedia", isDirectory: true)
      .appendingPathComponent(ownedDirectory.lastPathComponent, isDirectory: true)
    let aliasURL = aliasDirectory.appendingPathComponent(ownedURL.lastPathComponent)
    for cacheSpelling in [cache, aliasCache] {
      let accepted = try ZoenPreviewFile.validated(url: aliasURL, directory: aliasDirectory, cacheRoot: cacheSpelling, maximumBytes: 4096)
      precondition(accepted.url == plain.url && accepted.directory == plain.directory)
    }

    reject(ownedURL, directory: siblingDirectory)
    let nestedURL = try write(ownedDirectory.appendingPathComponent("nested", isDirectory: true))
    reject(nestedURL, directory: ownedDirectory)
    let outsideDirectory = sandbox.appendingPathComponent("OutsideNamespace", isDirectory: true)
      .appendingPathComponent(UUID().uuidString, isDirectory: true)
    let outsideURL = try write(outsideDirectory)
    reject(outsideURL, directory: outsideDirectory)
    let nonGeneratedDirectory = namespace.appendingPathComponent("not-a-generated-uuid", isDirectory: true)
    let nonGeneratedURL = try write(nonGeneratedDirectory)
    reject(nonGeneratedURL, directory: nonGeneratedDirectory)

    let leafLink = ownedDirectory.appendingPathComponent("external-original.pdf")
    try manager.createSymbolicLink(at: leafLink, withDestinationURL: externalURL)
    reject(leafLink, directory: ownedDirectory)
    // Point at another otherwise valid namespace leaf so rejection specifically
    // exercises the generated-root link check, not just namespace containment.
    let rootLink = namespace.appendingPathComponent(UUID().uuidString, isDirectory: true)
    try manager.createSymbolicLink(at: rootLink, withDestinationURL: siblingDirectory)
    reject(rootLink.appendingPathComponent(siblingURL.lastPathComponent), directory: rootLink)
    reject(ownedURL, directory: ownedDirectory, maximumBytes: 1)
    let emptyURL = try write(ownedDirectory, name: "empty.pdf", bytes: Data())
    reject(emptyURL, directory: ownedDirectory)

    try plain.remove()
    precondition(!manager.fileExists(atPath: ownedDirectory.path))
    let preservedExternalBytes = try Data(contentsOf: externalURL)
    precondition(preservedExternalBytes == externalBytes, "Lease cleanup followed a link into an external original")
    precondition(manager.fileExists(atPath: siblingURL.path), "Lease cleanup deleted another generated directory")
    precondition(manager.fileExists(atPath: outsideURL.path))
    precondition(manager.fileExists(atPath: nonGeneratedURL.path))
    let aliasType = try manager.attributesOfItem(atPath: aliasCache.path)[.type] as? FileAttributeType
    precondition(aliasType == .typeSymbolicLink)
  }
}
