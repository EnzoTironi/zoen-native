import Foundation

/// A restartable operation intent, never another account store. It contains no keys or content.
struct ZoenDeviceScope: Codable, Equatable, Sendable {
  let identityId: String
  let deviceId: String
  init(identityId: String, deviceId: String) throws {
    let valid = { (value: String) in value.count == 64 && value.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) } }
    guard valid(identityId), valid(deviceId) else { throw ZoenDeviceErasure.Failure.changed }
    self.identityId = identityId
    self.deviceId = deviceId
  }
}

final class ZoenDeviceErasure {
  static var accountRoot: URL { URL.applicationSupportDirectory.appending(path: "Roda", directoryHint: .isDirectory) }
  private let root: URL
  init(root: URL = ZoenDeviceErasure.accountRoot) { self.root = root }
  private var marker: URL { root.appending(path: "erase-device-v1.json") }
  private struct Intent: Codable { let schemaVersion: Int; let scope: ZoenDeviceScope }
  enum Failure: Error, LocalizedError {
    case changed, incomplete
    var errorDescription: String? {
      switch self {
      case .changed: "A conta mudou. Abra a confirmação novamente."
      case .incomplete: "Não foi possível concluir a saída deste aparelho. Tente novamente."
      }
    }
  }
  func pending() throws -> ZoenDeviceScope? {
    guard FileManager.default.fileExists(atPath: marker.path) else { return nil }
    let attributes = try FileManager.default.attributesOfItem(atPath: marker.path)
    guard let size = attributes[.size] as? NSNumber, size.intValue <= 2048 else { throw Failure.incomplete }
    let intent = try JSONDecoder().decode(Intent.self, from: Data(contentsOf: marker))
    guard intent.schemaVersion == 1 else { throw Failure.incomplete }
    // Codable decoding must receive the same validation as a fresh bridge request.
    return try ZoenDeviceScope(identityId: intent.scope.identityId, deviceId: intent.scope.deviceId)
  }
  func begin(_ scope: ZoenDeviceScope) throws {
    if let existing = try pending(), existing != scope { throw Failure.changed }
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    var directory = root
    var values = URLResourceValues(); values.isExcludedFromBackup = true
    try directory.setResourceValues(values)
    let data = try JSONEncoder().encode(Intent(schemaVersion: 1, scope: scope))
    try data.write(to: marker, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
  }
  func complete() throws {
    if FileManager.default.fileExists(atPath: marker.path) { try FileManager.default.removeItem(at: marker) }
  }
  static func erasePreviewCache() throws {
    let root = URL.cachesDirectory.appending(path: "ZoenMedia", directoryHint: .isDirectory)
    if FileManager.default.fileExists(atPath: root.path) { try FileManager.default.removeItem(at: root) }
  }
}
