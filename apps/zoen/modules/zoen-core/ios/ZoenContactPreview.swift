import Contacts
import CryptoKit
import Foundation

/// Read-only projection of one exact authorized vCard. No address-book access,
/// URL handling or imports; the caller retains the existing media-file lease.
enum ZoenContactPreview {
  static let maximumBytes = 64 * 1024
  struct Request: Decodable {
    let uri: String
    let itemId: String
    let version: UInt32
    let sha256: String

    static func decode(_ json: String) throws -> Self {
      guard json.utf8.count <= 4096 else { throw Failure.invalid }
      let value: Self
      do { value = try JSONDecoder().decode(Self.self, from: Data(json.utf8)) }
      catch { throw Failure.invalid }
      guard !value.uri.isEmpty, value.uri.utf8.count <= 2048,
            !value.itemId.isEmpty, value.itemId.utf8.count <= 256,
            value.version > 0, value.sha256.utf8.count == 64,
            value.sha256.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }) else {
        throw Failure.invalid
      }
      return value
    }
    func matches(itemId: String, version: UInt32, sha256: String, mime: String) -> Bool {
      self.itemId == itemId && self.version == version && self.sha256 == sha256 && ZoenContactPreview.supports(mime)
    }
  }
  struct Phone: Encodable, Equatable { let label: String; let value: String }
  struct Contact: Encodable, Equatable { let name: String; let phones: [Phone] }
  enum Failure: Error, LocalizedError {
    case invalid
    var errorDescription: String? {
      NSLocalizedString("This contact could not be opened.", comment: "Invalid or unavailable contact preview")
    }
  }
  static func supports(_ mime: String) -> Bool {
    let type = mime.split(separator: ";", maxSplits: 1).first?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    return type == "text/vcard" || type == "text/x-vcard"
  }
  static func read(_ url: URL, expectedHash: String) throws -> Contact {
    guard url.isFileURL else { throw Failure.invalid }
    let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
    guard values.isRegularFile == true, let size = values.fileSize,
          size > 0, size <= maximumBytes else { throw Failure.invalid }
    let handle = try FileHandle(forReadingFrom: url)
    defer { try? handle.close() }
    // Bound the actual read too: a file can change after its size preflight.
    guard let bytes = try handle.read(upToCount: maximumBytes + 1),
          bytes.count == size, bytes.count <= maximumBytes else { throw Failure.invalid }
    return try parse(bytes, expectedHash: expectedHash)
  }
  static func parse(_ bytes: Data, expectedHash: String) throws -> Contact {
    guard !bytes.isEmpty, bytes.count <= maximumBytes,
          SHA256.hash(data: bytes).map({ String(format: "%02x", $0) }).joined() == expectedHash else {
      throw Failure.invalid
    }
    let contacts: [CNContact]
    do { contacts = try CNContactVCardSerialization.contacts(with: bytes) }
    catch { throw Failure.invalid }
    guard contacts.count == 1, let contact = contacts.first,
          let name = CNContactFormatter.string(from: contact, style: .fullName)?.trimmingCharacters(in: .whitespacesAndNewlines),
          !name.isEmpty, name.utf8.count <= 256, !name.contains("\0"),
          contact.phoneNumbers.count <= 10 else { throw Failure.invalid }
    let phones = try contact.phoneNumbers.map { phone -> Phone in
      let value = phone.value.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
      let label = phone.label.map { CNLabeledValue<CNPhoneNumber>.localizedString(forLabel: $0) } ?? ""
      guard !value.isEmpty, value.utf8.count <= 80, label.utf8.count <= 64,
            !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains),
            !label.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains) else { throw Failure.invalid }
      return Phone(label: label, value: value)
    }
    return Contact(name: name, phones: phones)
  }
}
