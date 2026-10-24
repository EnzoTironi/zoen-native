import CryptoKit
import Foundation

@main struct LocationPreviewTests {
  static func hash(_ value: Data) -> String { SHA256.hash(data: value).map { String(format: "%02x", $0) }.joined() }
  static func denied(_ body: () throws -> Void) {
    do { try body(); preconditionFailure("Invalid location was accepted") } catch {}
  }
  static func main() throws {
    let good = Data(#"{"type":"Feature","geometry":{"type":"Point","coordinates":[-180,90]},"properties":{"label":"Café 東京 🌍"}}"#.utf8)
    let point = try ZoenStaticLocation.parse(good, expectedHash: hash(good))
    precondition(point.longitude == -180 && point.latitude == 90 && point.label == "Café 東京 🌍")
    let emptyLabel = Data(#"{"type":"Feature","geometry":{"type":"Point","coordinates":[180,-90]},"properties":{}}"#.utf8)
    let unlabelled = try ZoenStaticLocation.parse(emptyLabel, expectedHash: hash(emptyLabel))
    precondition(unlabelled.label == nil)
    for bad in [
      #"{"type":"Point","coordinates":[1,2]}"#,
      #"{"type":"FeatureCollection","features":[]}"#,
      #"{"type":"Feature","geometry":{"type":"MultiPoint","coordinates":[[1,2]]},"properties":{}}"#,
      #"{"type":"Feature","geometry":{"type":"Point","coordinates":[true,2]},"properties":{}}"#,
      #"{"type":"Feature","geometry":{"type":"Point","coordinates":[1,2,3]},"properties":{}}"#,
      #"{"type":"Feature","geometry":{"type":"Point","coordinates":[1,91]},"properties":{}}"#,
      #"{"type":"Feature","geometry":{"type":"Point","coordinates":[181,2]},"properties":{}}"#,
      #"{"type":"Feature","geometry":{"type":"Point","coordinates":[1e500,2]},"properties":{}}"#,
      #"{"type":"Feature","geometry":{"type":"Point","coordinates":[1,2]},"properties":{"label":null}}"#,
      #"{"type":"Feature","geometry":{"type":"Point","coordinates":[1,2]},"properties":{"label":" "}}"#,
      #"{"type":"Feature","geometry":{"type":"Point","coordinates":[1,2]},"properties":{"label":"x\n"}}"#,
      #"{"type":"Feature","geometry":{"type":"Point","coordinates":[1,2]},"properties":{"url":"https://invalid.test"}}"#,
      #"{"type":"Feature","geometry":{"type":"Point","coordinates":[1,2]},"properties":{},"crs":{}}"#,
      String(repeating: " ", count: ZoenStaticLocation.maximumBytes + 1)
    ] {
      let bytes = Data(bad.utf8)
      denied { _ = try ZoenStaticLocation.parse(bytes, expectedHash: hash(bytes)) }
    }
    denied { _ = try ZoenStaticLocation.parse(good, expectedHash: String(repeating: "0", count: 64)) }
    for value in [Double.infinity, -.infinity, .nan] {
      denied { _ = try ZoenStaticLocation.Point(longitude: value, latitude: 0, label: nil).validated() }
    }
    denied { _ = try ZoenStaticLocation.Point(longitude: 0, latitude: 0, label: String(repeating: "é", count: 129)).validated() }
    let joinedEmoji = "Família 👨‍👩‍👧‍👦 / مکان"
    let joined = try ZoenStaticLocation.Point(longitude: 0, latitude: 0, label: joinedEmoji).validated()
    precondition(joined.label == joinedEmoji, "Unicode joiners in real place names must round-trip exactly")
    let request = try ZoenStaticLocation.Request.decode("{\"uri\":\"file:///synthetic/location.geojson\",\"itemId\":\"i\",\"version\":1,\"sha256\":\"\(hash(good))\"}")
    precondition(request.matches(itemId: "i", version: 1, sha256: hash(good), mime: "application/geo+json; charset=utf-8"))
    precondition(!request.matches(itemId: "other", version: 1, sha256: hash(good), mime: "application/geo+json"))
    precondition(!request.matches(itemId: "i", version: 2, sha256: hash(good), mime: "application/geo+json"))
    precondition(!request.matches(itemId: "i", version: 1, sha256: hash(good), mime: "text/plain"))
    for bad in ["{}", String(repeating: " ", count: 4097)] { denied { _ = try ZoenStaticLocation.Request.decode(bad) } }
    let root = URL.temporaryDirectory.appendingPathComponent("zoen-location-test-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let file = root.appendingPathComponent("location.geojson")
    try good.write(to: file)
    let loaded = try ZoenStaticLocation.read(file, expectedHash: hash(good))
    precondition(loaded == point)
    try Data("changed".utf8).write(to: file)
    denied { _ = try ZoenStaticLocation.read(file, expectedHash: hash(good)) }
    try Data(repeating: 65, count: 4097).write(to: file)
    denied { _ = try ZoenStaticLocation.read(file, expectedHash: hash(good)) }
    print("Location parser PASS: actual bounded/hash GeoJSON parser, strict one-Point schema/Unicode/coordinates/file mutation/request binding; no network or UI acceptance.")
  }
}
