import Foundation

/// Display binding for a cache copy already held by the existing draft presenter.
/// It is not a canonical File ID, file permission or a separate draft store.
struct ZoenDraftLocationSource: Equatable {
  let sha256: String
  static func decode(_ value: [String: Any], source: String) throws -> Self? {
    guard source == "draft", let mime = value["mime"] as? String,
          ZoenStaticLocation.supports(mime) else { return nil }
    guard let hash = value["sha256"] as? String, hash.utf8.count == 64,
          hash.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }) else {
      throw ZoenStaticLocation.Failure.invalid
    }
    return Self(sha256: hash)
  }
  func read(_ retainedURL: URL) throws -> ZoenStaticLocation.Point {
    try ZoenStaticLocation.read(retainedURL, expectedHash: sha256)
  }
}
