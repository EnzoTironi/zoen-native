import Foundation
import Security
import RodaCore

/// Where this device's account keys live: the Keychain, never the SQLite file.
///
/// iPhone: a generic password, readable after the first unlock, never synced to iCloud
/// and never restored to another device (a new device is a new device key).
/// Mac: the data-protection keychain when the build is entitled to it; an unsigned dev
/// build isn't, so it falls back to a 0600 file under Application Support (dev only,
/// the same model as the `zoen` CLI).
final class KeychainVault: SecretVault, @unchecked Sendable {
    private let service: String
    private let lock = NSLock()
    private var useFile = false

    init(service: String = "xyz.tironi.zoen.keys") {
        self.service = service
    }

    func load(key: String) -> Data? {
        lock.lock(); defer { lock.unlock() }
        if !useFile {
            var q = base(key)
            q[kSecReturnData as String] = true
            q[kSecMatchLimit as String] = kSecMatchLimitOne
            var out: CFTypeRef?
            let status = SecItemCopyMatching(q as CFDictionary, &out)
            if status == errSecSuccess { return out as? Data }
            if !fallsBack(status) { return nil }
        }
        return try? Data(contentsOf: fileURL(key))
    }

    func save(key: String, value: Data) -> Bool {
        lock.lock(); defer { lock.unlock() }
        if !useFile {
            SecItemDelete(base(key) as CFDictionary)
            var q = base(key)
            q[kSecValueData as String] = value
            q[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
            let status = SecItemAdd(q as CFDictionary, nil)
            if status == errSecSuccess { return true }
            if !fallsBack(status) { return false }
        }
        let url = fileURL(key)
        do {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
            try value.write(to: url, options: [.atomic, .completeFileProtection])
            try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
            return true
        } catch {
            return false
        }
    }

    func delete(key: String) {
        lock.lock(); defer { lock.unlock() }
        SecItemDelete(base(key) as CFDictionary)
        try? FileManager.default.removeItem(at: fileURL(key))
    }

    private func base(_ key: String) -> [String: Any] {
        var q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
            kSecAttrSynchronizable as String: false,
        ]
        #if os(macOS)
        q[kSecUseDataProtectionKeychain as String] = true
        #endif
        return q
    }

    /// Only an unentitled Mac dev build falls back to the file.
    private func fallsBack(_ status: OSStatus) -> Bool {
        #if os(macOS)
        if status == errSecMissingEntitlement {
            useFile = true
            return true
        }
        #endif
        return false
    }

    private func fileURL(_ key: String) -> URL {
        let safe = key.map { $0.isLetter || $0.isNumber || $0 == "-" || $0 == "_" ? $0 : "_" }
        return URL.applicationSupportDirectory
            .appending(path: "Zoen/keys", directoryHint: .isDirectory)
            .appending(path: String(safe))
    }
}
