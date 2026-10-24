import CryptoKit
import Foundation

// Run the actual production ZoenSession extension and Contacts parser. Only
// opaque Rust authorization/current-session and private-file ownership ports
// are replaced; these tests are not Rust permission or OS lease acceptance.
enum SessionError: Error { case closed }
struct ContactFileInfo { let sha256: String; let mime: String; let bytes: UInt64 }
final class ContactEngine {
  var calls = 0
  var denyAt: Int?
  var info: ContactFileInfo
  init(_ info: ContactFileInfo) { self.info = info }
  func fileInfoAt(itemId: String, version: UInt32) throws -> ContactFileInfo {
    calls += 1
    if calls == denyAt { throw SessionError.closed }
    return info
  }
}
final class ContactFiles {
  var owned: [String: URL] = [:]
  func ownedSelectionURL(_ uri: String) throws -> URL {
    guard let url = owned[uri] else { throw SessionError.closed }
    return url
  }
}
actor ZoenSession {
  struct Binding {
    let itemId: String; let version: UInt32; let sha256: String; let mime: String
    let engine: ContactEngine
  }
  var erasing = false, closing = false
  let mediaFiles = ContactFiles()
  var previewBindings: [String: Binding] = [:]
  var checks = 0, denyCurrentAt: Int?
  func bindingIsCurrent(_ binding: Binding) -> Bool {
    checks += 1
    return checks != denyCurrentAt
  }
  func exercise() throws {
    let data = Data("BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Ana\r\nN:;Ana;;;\r\nTEL:123\r\nEND:VCARD\r\n".utf8)
    let hash = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    let root = URL.temporaryDirectory.appendingPathComponent("zoen-contact-scope-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let url = root.appendingPathComponent("contact.vcf"), uri = url.absoluteString
    let engine = ContactEngine(.init(sha256: hash, mime: "text/vcard", bytes: UInt64(data.count)))
    let binding = Binding(itemId: "item", version: 7, sha256: hash, mime: "text/vcard", engine: engine)
    func request(item: String = "item", version: UInt32 = 7, sha: String? = nil) -> String {
      "{\"uri\":\"\(uri)\",\"itemId\":\"\(item)\",\"version\":\(version),\"sha256\":\"\(sha ?? hash)\"}"
    }
    func denied(_ action: () throws -> String) {
      do { _ = try action(); preconditionFailure("Contact gate admitted a retired or mismatched source") } catch {}
    }
    try data.write(to: url)
    mediaFiles.owned[uri] = url; previewBindings[url.path] = binding
    let object = try JSONSerialization.jsonObject(with: Data(readContactFile(request()).utf8)) as? [String: Any]
    precondition(object?["name"] as? String == "Ana" && engine.calls == 2)
    for bad in [request(item: "foreign"), request(version: 8), request(sha: String(repeating: "0", count: 64))] {
      engine.calls = 0; denied { try readContactFile(bad) }; precondition(engine.calls == 0)
    }
    closing = true; denied { try readContactFile(request()) }; closing = false
    erasing = true; denied { try readContactFile(request()) }; erasing = false
    checks = 0; denyCurrentAt = 1; denied { try readContactFile(request()) }
    checks = 0; denyCurrentAt = 2; denied { try readContactFile(request()) }; denyCurrentAt = nil
    engine.calls = 0; engine.denyAt = 1; denied { try readContactFile(request()) }
    precondition(engine.calls == 1, "initial Core read refusal cannot fall back to cached contact metadata")
    engine.calls = 0; engine.denyAt = 2; denied { try readContactFile(request()) }
    precondition(engine.calls == 2, "the current source permission is read again before returning")
    engine.denyAt = nil
    mediaFiles.owned.removeValue(forKey: uri); denied { try readContactFile(request()) }
    mediaFiles.owned[uri] = url
    try Data("tampered".utf8).write(to: url)
    denied { try readContactFile(request()) }
    try data.write(to: url)
    let restored = try JSONSerialization.jsonObject(with: Data(readContactFile(request()).utf8))
    precondition(restored is [String: Any])
    precondition(mediaFiles.owned[uri] == url, "read does not transfer or release the caller's lease")
    print("Contact session PASS: actual production extension binds held source, rejects foreign/version/hash/released/tampered/closed/erasing/current-scope/Core-read refusal, rechecks before returning and preserves lease; opaque ports replaced.")
  }
}
@main struct ContactSessionTests {
  static func main() async throws { try await ZoenSession().exercise() }
}
