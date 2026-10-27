import CryptoKit
import Foundation

@main struct DraftLocationSourceTests {
  static func denied(_ body: () throws -> Void) {
    do { try body(); preconditionFailure("Invalid draft location preview admitted") } catch {}
  }
  static func main() throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    let bytes = Data(#"{"type":"Feature","geometry":{"type":"Point","coordinates":[-43,-19]},"properties":{"label":"Café 👩‍💻"}}"#.utf8)
    let hash = SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
    let model: [String: Any] = ["mime": "application/geo+json", "sha256": hash]
    let source = try ZoenDraftLocationSource.decode(model, source: "draft")!
    let url = directory.appendingPathComponent("held.geojson")
    try bytes.write(to: url)
    let point = try source.read(url)
    precondition(point.longitude == -43 && point.latitude == -19 && point.label == "Café 👩‍💻")
    let conversation = try ZoenDraftLocationSource.decode(model, source: "conversation")
    precondition(conversation == nil)
    let generic = try ZoenDraftLocationSource.decode(["mime": "application/json", "sha256": hash], source: "draft")
    precondition(generic == nil)
    let invalid: [[String: Any]] = [["mime": "application/geo+json"], ["mime": "application/geo+json", "sha256": "bad"], ["mime": "application/geo+json", "sha256": String(repeating: "G", count: 64)]]
    for bad in invalid {
      denied { _ = try ZoenDraftLocationSource.decode(bad, source: "draft") }
    }
    let replacement = Data(#"{"type":"Feature","geometry":{"type":"Point","coordinates":[11,22]},"properties":{}}"#.utf8)
    try replacement.write(to: url)
    denied { _ = try source.read(url) }
    let oversized = Data(repeating: 32, count: 4097)
    try oversized.write(to: url)
    denied { _ = try source.read(url) }
    try FileManager.default.removeItem(at: url)
    denied { _ = try source.read(url) }
    print("Draft location held-copy/hash PASS")
  }
}
