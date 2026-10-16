#if DEBUG
import Foundation
import RodaCore

/// Fresh synthetic fixture keys only. Never reads Keychain, writes disk, or logs keys.
/// Its isolated fixture database is reset on every launch; no user account uses this.
final class LivingFixtureVault: SecretVault, @unchecked Sendable {
    private let lock = NSLock()
    private var values: [String: Data] = [:]

    func load(key: String) -> Data? {
        lock.lock(); defer { lock.unlock() }
        return values[key]
    }
    func save(key: String, value: Data) -> Bool {
        lock.lock(); defer { lock.unlock() }
        values[key] = value
        return true
    }
    func delete(key: String) {
        lock.lock(); defer { lock.unlock() }
        values.removeValue(forKey: key)
    }
}
#endif
