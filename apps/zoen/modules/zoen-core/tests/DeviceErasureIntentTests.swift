import Foundation

@main struct ErasureIntentTests {
  static func expect(_ value: ZoenDeviceScope?, _ expected: ZoenDeviceScope?) { precondition(value == expected) }
  static func main() throws {
    precondition(ZoenDeviceErasure.accountRoot.lastPathComponent == "account")
    precondition(ZoenDeviceErasure.accountRoot.deletingLastPathComponent() == URL.applicationSupportDirectory)
    let root = FileManager.default.temporaryDirectory.appendingPathComponent("zoen-erasure-fixture-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let a = try ZoenDeviceScope(identityId: String(repeating: "a", count: 64), deviceId: String(repeating: "b", count: 64))
    let b = try ZoenDeviceScope(identityId: String(repeating: "c", count: 64), deviceId: String(repeating: "d", count: 64))
    let journal = ZoenDeviceErasure(root: root)
    expect(try journal.pending(), nil)
    try journal.begin(a)
    expect(try ZoenDeviceErasure(root: root).pending(), a)
    try journal.begin(a)
    do { try journal.begin(b); fatalError("changed scope accepted") } catch ZoenDeviceErasure.Failure.changed {}
    expect(try journal.pending(), a)
    try journal.complete()
    expect(try journal.pending(), nil)
    try journal.begin(a)
    try FileManager.default.setAttributes([.posixPermissions: 0o000], ofItemAtPath: root.path)
    defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: root.path) }
    do { _ = try journal.pending(); fatalError("unreadable journal parent accepted as absent") }
    catch ZoenDeviceErasure.Failure.incomplete {}
    do { try journal.complete(); fatalError("unreadable journal claimed complete") }
    catch ZoenDeviceErasure.Failure.incomplete {}
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: root.path)
    expect(try journal.pending(), a)
    try journal.complete()
    expect(try journal.pending(), nil)
    do { _ = try ZoenDeviceScope(identityId: "invalid", deviceId: a.deviceId); fatalError("invalid scope accepted") } catch ZoenDeviceErasure.Failure.changed {}
    try Data(repeating: 65, count: 2049).write(to: root.appendingPathComponent("erase-device-v1.json"))
    do { _ = try journal.pending(); fatalError("oversized intent accepted") } catch ZoenDeviceErasure.Failure.incomplete {}
    print("Synthetic erasure intent tests passed: restart, idempotency, scope mismatch, completion, inaccessible parent/retry, bounded corrupt record.")
  }
}
