import CryptoKit
import Foundation

// Run the actual production ZoenSession extension and Locations parser. Only
// opaque Rust authorization/current-session and private-file ownership ports
// are replaced; these tests are not Rust permission or OS lease acceptance.
enum SessionError: Error { case closed }
struct LocationFileInfo { let sha256: String; let mime: String; let bytes: UInt64 }
struct LocationAccount { var identityId = "owner", deviceId = "device", unlocked = true, setupPending = false }
final class LocationEngine {
  var currentAccount = LocationAccount()
  func account() -> LocationAccount? { currentAccount }
  var calls = 0
  var denyAt: Int?
  var info: LocationFileInfo
  init(_ info: LocationFileInfo) { self.info = info }
  func fileInfoAt(itemId: String, version: UInt32) throws -> LocationFileInfo {
    calls += 1
    if calls == denyAt { throw SessionError.closed }
    return info
  }
}
final class LocationFiles {
  var owned: [String: URL] = [:]
  func ownedSelectionURL(_ uri: String) throws -> URL {
    guard let url = owned[uri] else { throw SessionError.closed }
    return url
  }
}
actor ZoenSession {
  struct Binding {
    let itemId: String; let version: UInt32; let sha256: String; let mime: String
    let engine: LocationEngine
  }
  var erasing = false, closing = false
  var core: LocationEngine?
  var mode = "account"
  var generation: UInt64 = 1
  let mediaFiles = LocationFiles()
  var previewBindings: [String: Binding] = [:]
  var checks = 0, denyCurrentAt: Int?
  func bindingIsCurrent(_ binding: Binding) -> Bool {
    checks += 1
    return checks != denyCurrentAt
  }
  func exercise() throws {
    let data = Data(#"{"type":"Feature","geometry":{"type":"Point","coordinates":[1,2]},"properties":{"label":"Ana"}}"#.utf8)
    let hash = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    let root = URL.temporaryDirectory.appendingPathComponent("zoen-contact-scope-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let url = root.appendingPathComponent("location.geojson"), uri = url.absoluteString
    let engine = LocationEngine(.init(sha256: hash, mime: "application/geo+json", bytes: UInt64(data.count)))
    core = engine
    let owner = try locationSearchScope()
    try requireLocationSearchScope(owner)
    generation += 1
    denied { try requireLocationSearchScope(owner); return "" }
    generation -= 1
    engine.currentAccount.deviceId = "other"
    denied { try requireLocationSearchScope(owner); return "" }
    engine.currentAccount.deviceId = "device"
    engine.currentAccount.unlocked = false
    denied { try requireLocationSearchScope(owner); return "" }
    engine.currentAccount.unlocked = true
    let binding = Binding(itemId: "item", version: 7, sha256: hash, mime: "application/geo+json", engine: engine)
    func request(item: String = "item", version: UInt32 = 7, sha: String? = nil) -> String {
      "{\"uri\":\"\(uri)\",\"itemId\":\"\(item)\",\"version\":\(version),\"sha256\":\"\(sha ?? hash)\"}"
    }
    func denied(_ action: () throws -> String) {
      do { _ = try action(); preconditionFailure("Location gate admitted a retired or mismatched source") } catch {}
    }
    try data.write(to: url)
    mediaFiles.owned[uri] = url; previewBindings[url.path] = binding
    let object = try JSONSerialization.jsonObject(with: Data(readLocationFile(request()).utf8)) as? [String: Any]
    precondition(object?["label"] as? String == "Ana" && engine.calls == 2)
    for bad in [request(item: "foreign"), request(version: 8), request(sha: String(repeating: "0", count: 64))] {
      engine.calls = 0; denied { try readLocationFile(bad) }; precondition(engine.calls == 0)
    }
    closing = true; denied { try readLocationFile(request()) }; closing = false
    erasing = true; denied { try readLocationFile(request()) }; erasing = false
    checks = 0; denyCurrentAt = 1; denied { try readLocationFile(request()) }
    checks = 0; denyCurrentAt = 2; denied { try readLocationFile(request()) }; denyCurrentAt = nil
    engine.calls = 0; engine.denyAt = 1; denied { try readLocationFile(request()) }
    precondition(engine.calls == 1, "initial Core read refusal cannot fall back to cached contact metadata")
    engine.calls = 0; engine.denyAt = 2; denied { try readLocationFile(request()) }
    precondition(engine.calls == 2, "the current source permission is read again before returning")
    engine.denyAt = nil
    mediaFiles.owned.removeValue(forKey: uri); denied { try readLocationFile(request()) }
    mediaFiles.owned[uri] = url
    try Data("tampered".utf8).write(to: url)
    denied { try readLocationFile(request()) }
    try data.write(to: url)
    let restored = try JSONSerialization.jsonObject(with: Data(readLocationFile(request()).utf8))
    precondition(restored is [String: Any])
    precondition(mediaFiles.owned[uri] == url, "read does not transfer or release the caller's lease")
    print("Location session PASS: actual production location extension binds held source, rejects foreign/version/hash/released/tampered/closed/erasing/current-scope/Core-read refusal, rechecks before returning and preserves lease; opaque ports replaced.")
  }
}
@main struct LocationSessionTests {
  static func main() async throws { try await ZoenSession().exercise() }
}
