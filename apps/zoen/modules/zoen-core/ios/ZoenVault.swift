import Foundation
import Security

/// Account material stays in this device's nonsynchronizing Keychain.
final class ZoenVault: SecretVault, @unchecked Sendable {
  private let service = "xyz.tironi.zoen.keys"
  private let lock = NSLock()

  private func query(_ key: String) -> [String: Any] {
    [kSecClass as String: kSecClassGenericPassword,
     kSecAttrService as String: service,
     kSecAttrAccount as String: key,
     kSecAttrSynchronizable as String: false]
  }

  func hasAccountMaterial() -> Bool {
    lock.lock(); defer { lock.unlock() }
    return ["zoen.identity.v1", "zoen.device.v1"].contains {
      // Any result other than a definite absence must preserve the existing vault.
      SecItemCopyMatching(query($0) as CFDictionary, nil) != errSecItemNotFound
    }
  }

  func load(key: String) -> Data? {
    lock.lock(); defer { lock.unlock() }
    var request = query(key)
    request[kSecReturnData as String] = true
    var result: CFTypeRef?
    guard SecItemCopyMatching(request as CFDictionary, &result) == errSecSuccess else { return nil }
    return result as? Data
  }

  func save(key: String, value: Data) -> Bool {
    lock.lock(); defer { lock.unlock() }
    let update = [kSecValueData as String: value]
    let status = SecItemUpdate(query(key) as CFDictionary, update as CFDictionary)
    if status == errSecSuccess { return true }
    guard status == errSecItemNotFound else { return false }
    var item = query(key)
    item[kSecValueData as String] = value
    item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    return SecItemAdd(item as CFDictionary, nil) == errSecSuccess
  }

  func delete(key: String) {
    lock.lock(); defer { lock.unlock() }
    SecItemDelete(query(key) as CFDictionary)
  }
}
