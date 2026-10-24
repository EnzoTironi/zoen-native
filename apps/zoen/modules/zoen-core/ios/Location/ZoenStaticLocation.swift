import CryptoKit
import Foundation

/// One bounded, inert GeoJSON Point; no URL/geocoder/GPS authority is encoded.
enum ZoenStaticLocation {
  static let maximumBytes = 4096
  static func hasControls(_ value: String) -> Bool {
    value.unicodeScalars.contains { $0.value < 32 || (127...159).contains($0.value) }
  }
  struct Point: Codable, Equatable, Sendable {
    let longitude: Double
    let latitude: Double
    let label: String?

    func validated() throws -> Self {
      guard longitude.isFinite, latitude.isFinite,
            (-180...180).contains(longitude), (-90...90).contains(latitude) else { throw Failure.invalid }
      if let label {
        guard !label.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              label.utf8.count <= 256,
              !ZoenStaticLocation.hasControls(label) else { throw Failure.invalid }
      }
      return self
    }
  }
  enum Failure: Error { case invalid, retired }
  struct Request: Decodable {
    let uri: String
    let itemId: String
    let version: UInt32
    let sha256: String
    static func decode(_ json: String) throws -> Self {
      guard json.utf8.count <= 4096 else { throw Failure.invalid }
      let value = try JSONDecoder().decode(Self.self, from: Data(json.utf8))
      guard !value.uri.isEmpty, value.uri.utf8.count <= 2048,
            !value.itemId.isEmpty, value.itemId.utf8.count <= 256, value.version > 0,
            value.sha256.utf8.count == 64,
            value.sha256.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }) else { throw Failure.invalid }
      return value
    }
    func matches(itemId: String, version: UInt32, sha256: String, mime: String) -> Bool {
      self.itemId == itemId && self.version == version && self.sha256 == sha256 && ZoenStaticLocation.supports(mime)
    }
  }
  static func supports(_ mime: String) -> Bool {
    mime.split(separator: ";", maxSplits: 1).first?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() == "application/geo+json"
  }
  static func read(_ url: URL, expectedHash: String) throws -> Point {
    guard url.isFileURL else { throw Failure.invalid }
    let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
    guard values.isRegularFile == true, let size = values.fileSize, size > 0, size <= maximumBytes else { throw Failure.invalid }
    let handle = try FileHandle(forReadingFrom: url)
    defer { try? handle.close() }
    guard let bytes = try handle.read(upToCount: maximumBytes + 1), bytes.count == size,
          bytes.count <= maximumBytes else { throw Failure.invalid }
    return try parse(bytes, expectedHash: expectedHash)
  }
  static func parse(_ bytes: Data, expectedHash: String) throws -> Point {
    guard !bytes.isEmpty, bytes.count <= maximumBytes,
          SHA256.hash(data: bytes).map({ String(format: "%02x", $0) }).joined() == expectedHash else { throw Failure.invalid }
    return try JSONDecoder().decode(Feature.self, from: bytes).point.validated()
  }
  private struct Key: CodingKey {
    let stringValue: String
    let intValue: Int? = nil
    init?(stringValue: String) { self.stringValue = stringValue }
    init?(intValue: Int) { return nil }
  }
  private static func fields(_ decoder: Decoder, allowed: Set<String>) throws {
    let container = try decoder.container(keyedBy: Key.self)
    guard Set(container.allKeys.map(\.stringValue)).isSubset(of: allowed) else { throw Failure.invalid }
  }
  private struct Feature: Decodable {
    let point: Point
    enum CodingKeys: String, CodingKey { case type, geometry, properties }
    init(from decoder: Decoder) throws {
      try fields(decoder, allowed: ["type", "geometry", "properties"])
      let c = try decoder.container(keyedBy: CodingKeys.self)
      guard try c.decode(String.self, forKey: .type) == "Feature" else { throw Failure.invalid }
      let geometry = try c.decode(Geometry.self, forKey: .geometry)
      let properties = try c.decode(Properties.self, forKey: .properties)
      point = Point(longitude: geometry.longitude, latitude: geometry.latitude, label: properties.label)
    }
  }
  private struct Geometry: Decodable {
    let longitude: Double
    let latitude: Double
    enum CodingKeys: String, CodingKey { case type, coordinates }
    init(from decoder: Decoder) throws {
      try fields(decoder, allowed: ["type", "coordinates"])
      let c = try decoder.container(keyedBy: CodingKeys.self)
      guard try c.decode(String.self, forKey: .type) == "Point" else { throw Failure.invalid }
      let coordinates = try c.decode([Double].self, forKey: .coordinates)
      guard coordinates.count == 2 else { throw Failure.invalid }
      longitude = coordinates[0]; latitude = coordinates[1]
    }
  }
  private struct Properties: Decodable {
    let label: String?
    enum CodingKeys: String, CodingKey { case label }
    init(from decoder: Decoder) throws {
      try fields(decoder, allowed: ["label"])
      let c = try decoder.container(keyedBy: CodingKeys.self)
      label = c.contains(.label) ? try c.decode(String.self, forKey: .label) : nil
    }
  }
}
