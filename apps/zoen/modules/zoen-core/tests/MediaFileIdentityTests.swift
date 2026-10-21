import Foundation

/// Executes actual ZoenMediaFiles and ZoenPreviewFile normalization/ownership.
/// Rust authorization/CAS and UIKit presentation are deliberately outside scope.
@main struct MediaFileIdentityTests {
  static let bytes = Data("synthetic authorized source bytes".utf8)
  static func denied(_ action: () throws -> Void) {
    do { try action(); preconditionFailure("Retired or substituted lease was accepted") }
    catch {}
  }
  static func main() throws {
    let manager = FileManager.default
    let base = URL.temporaryDirectory.appendingPathComponent("zoen-media-identity-\(UUID().uuidString)", isDirectory: true)
    let cache = base.appendingPathComponent("cache", isDirectory: true)
    let temporary = base.appendingPathComponent("tmp", isDirectory: true)
    try manager.createDirectory(at: cache, withIntermediateDirectories: true)
    try manager.createDirectory(at: temporary, withIntermediateDirectories: true)
    defer { try? manager.removeItem(at: base) }
    let media = ZoenMediaFiles(cacheRoot: cache, temporaryRoot: temporary)
    let uri = try media.stage(bytes, name: "alpha.pdf")
    let original = try ZoenMediaFiles.selectionURL(uri)
    let sourceIdentity = try media.ownedSelectionURL(uri).path
    let provenance = [sourceIdentity: "exact source scope/version/hash"]
    let link = temporary.appendingPathComponent("source-link.pdf")
    try manager.createSymbolicLink(at: link, withDestinationURL: original)
    let ancestor = base.appendingPathComponent("cache-alias", isDirectory: true)
    try manager.createSymbolicLink(at: ancestor, withDestinationURL: cache)
    let aliasSpelling = original.path.replacingOccurrences(of: cache.resolvingSymlinksInPath().path, with: ancestor.path)
    let aliases = [uri, uri.replacingOccurrences(of: "alpha.pdf", with: "%61lpha.pdf"), original.deletingLastPathComponent().appendingPathComponent("spare/../alpha.pdf").absoluteString, uri + "?source=alternate#preview", link.absoluteString, URL(fileURLWithPath: aliasSpelling).absoluteString]
    for alias in aliases {
      let selection = try media.readSelection(uri: alias, name: "alpha.pdf", mime: "application/pdf")
      precondition(selection.data == bytes && provenance[selection.url.path] == "exact source scope/version/hash")
      let identity = try media.ownedSelectionURL(alias).path
      precondition(identity == sourceIdentity)
    }
    let caseAlias = uri.replacingOccurrences(of: "ZoenMedia", with: "zoenmedia").replacingOccurrences(of: "alpha.pdf", with: "ALPHA.PDF")
    do {
      let selection = try media.readSelection(uri: caseAlias, name: "alpha.pdf", mime: "application/pdf")
      precondition(provenance[selection.url.path] != nil, "Case alias bypassed provenance")
    } catch {} // Unnormalized filesystem spellings may be rejected, never downgraded.

    // A transferred preview still physically exists but no longer authorizes
    // ordinary ingestion, through either canonical or alternative spellings.
    let transfer = try media.takeForPreview(link.absoluteString)
    precondition(manager.fileExists(atPath: transfer.url.path))
    for alias in aliases { denied { _ = try media.readSelection(uri: alias, name: "alpha.pdf", mime: "application/pdf") } }
    let captured = try media.readCapturedSelection(transfer, name: "alpha.pdf", mime: "application/pdf")
    precondition(captured.data == bytes, "Typed native PDF/capture owner lost its existing save admission")
    let forged = ZoenPreviewFile(url: temporary.appendingPathComponent("outside.pdf"), directory: transfer.directory)
    denied { _ = try media.readCapturedSelection(forged, name: "outside.pdf", mime: "application/pdf") }
    try transfer.remove()

    // Alias release removes the same lease. Simulate leftover bytes after a
    // denied OS deletion: the retired namespace must still fail closed.
    let releaseURI = try media.stage(bytes, name: "release.pdf")
    let releaseURL = try ZoenMediaFiles.selectionURL(releaseURI)
    let encodedRelease = releaseURI.replacingOccurrences(of: "release.pdf", with: "%72elease.pdf")
    media.release(encodedRelease)
    precondition(!manager.fileExists(atPath: releaseURL.path))
    try manager.createDirectory(at: releaseURL.deletingLastPathComponent(), withIntermediateDirectories: true)
    try bytes.write(to: releaseURL)
    for alias in [releaseURI, encodedRelease, releaseURI + "#stale"] { denied { _ = try media.readSelection(uri: alias, name: "release.pdf", mime: "application/pdf") } }
    let canonicalRelease = try media.stage(bytes, name: "canonical.pdf")
    let canonicalURL = try ZoenMediaFiles.selectionURL(canonicalRelease)
    media.release(canonicalRelease)
    precondition(!manager.fileExists(atPath: canonicalURL.path))
    try manager.createDirectory(at: canonicalURL.deletingLastPathComponent(), withIntermediateDirectories: true)
    try bytes.write(to: canonicalURL)
    denied { _ = try media.readSelection(uri: canonicalRelease + "?retired", name: "canonical.pdf", mime: "application/pdf") }

    let ordinary = cache.appendingPathComponent("ordinary.pdf")
    let ordinaryTemporary = temporary.appendingPathComponent("ordinary.pdf")
    try bytes.write(to: ordinary); try bytes.write(to: ordinaryTemporary)
    for file in [ordinary, ordinaryTemporary] {
      let selection = try media.readSelection(uri: file.absoluteString, name: "ordinary.pdf", mime: "application/pdf")
      precondition(selection.data == bytes)
    }
    // A lexical managed path replaced by a symlink cannot escape into the
    // ordinary-picker fallback, nor can releasing it delete the unrelated file.
    let substituted = try media.stage(bytes, name: "substituted.pdf")
    let substitutedURL = try ZoenMediaFiles.selectionURL(substituted)
    try manager.removeItem(at: substitutedURL)
    try manager.createSymbolicLink(at: substitutedURL, withDestinationURL: ordinary)
    denied { _ = try media.readSelection(uri: substituted, name: "substituted.pdf", mime: "application/pdf") }
    media.release(substituted)
    precondition(manager.fileExists(atPath: ordinary.path))
    precondition(media.releaseIdentity(substituted) == nil)
    let first = try media.stage(bytes, name: "first.pdf"), second = try media.stage(bytes, name: "second.pdf")
    media.removeAll()
    let firstURL = try ZoenMediaFiles.selectionURL(first), secondURL = try ZoenMediaFiles.selectionURL(second)
    precondition(!manager.fileExists(atPath: firstURL.path))
    precondition(!manager.fileExists(atPath: secondURL.path))
    precondition(manager.fileExists(atPath: ordinary.path) && manager.fileExists(atPath: ordinaryTemporary.path))
    print("Media identity PASS: canonical/percent/dot/query-fragment/symlink/ancestor aliases preserve provenance; case cannot downgrade; transfer/release leftovers rejected; substitution fenced; ordinary cache/tmp preserved; canonical removeAll.")
  }
}
