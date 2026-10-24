import Contacts
import CryptoKit
import Foundation

/// Actual Contacts parser and bounded file/digest adapter. No address book,
/// UIKit, Rust authorization, source delivery or iPhone acceptance is implied.
@main struct ContactPreviewTests {
  static func hash(_ bytes: Data) -> String { SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined() }
  static func denied(_ body: () throws -> Void) {
    do { try body(); preconditionFailure("Invalid contact was accepted") } catch {}
  }
  static func main() throws {
    let card = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Ana Silva\r\nN:Silva;Ana;;;\r\nTEL;TYPE=CELL:+55 11 1234\r\nEMAIL:private@example.invalid\r\nNOTE:must not be projected\r\nEND:VCARD\r\n"
    let bytes = Data(card.utf8), digest = hash(bytes)
    let contact = try ZoenContactPreview.parse(bytes, expectedHash: digest)
    precondition(contact.name.contains("Ana") && contact.name.contains("Silva"))
    precondition(contact.phones.count == 1 && contact.phones.first?.value == "+55 11 1234")
    let object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(contact)) as? [String: Any]
    precondition(Set(object?.keys.map { $0 } ?? []) == Set(["name", "phones"]))
    denied { _ = try ZoenContactPreview.parse(bytes, expectedHash: String(repeating: "0", count: 64)) }
    for bad in [Data(), Data("not a contact".utf8), bytes + bytes,
                Data(card.replacingOccurrences(of: "FN:Ana Silva\r\nN:Silva;Ana;;;\r\n", with: "").utf8),
                Data(repeating: 65, count: ZoenContactPreview.maximumBytes + 1),
                Data(card.replacingOccurrences(of: "TEL;TYPE=CELL:+55 11 1234", with: (0..<11).map { "TEL:123\($0)" }.joined(separator: "\r\n")).utf8),
                Data(card.replacingOccurrences(of: "Ana Silva", with: String(repeating: "é", count: 257)).replacingOccurrences(of: "Silva;Ana", with: String(repeating: "é", count: 257)).utf8)] {
      denied { _ = try ZoenContactPreview.parse(bad, expectedHash: hash(bad)) }
    }
    let folded = Data("BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Ana\\,\r\n  Silva\r\nN:;Ana\\, Silva;;;\r\nTEL:123\r\nEND:VCARD\r\n".utf8)
    let foldedContact = try ZoenContactPreview.parse(folded, expectedHash: hash(folded))
    precondition(foldedContact.name.contains("Ana,") && foldedContact.name.contains("Silva"))
    for mime in ["text/vcard", "text/x-vcard", "TEXT/VCARD; charset=utf-8"] { precondition(ZoenContactPreview.supports(mime)) }
    for mime in ["text/plain", "application/pdf", "text/directory", "application/octet-stream"] { precondition(!ZoenContactPreview.supports(mime)) }
    let request = try ZoenContactPreview.Request.decode("{\"uri\":\"file:///synthetic/contact.vcf\",\"itemId\":\"item\",\"version\":7,\"sha256\":\"\(digest)\"}")
    precondition(request.matches(itemId: "item", version: 7, sha256: digest, mime: "text/vcard"))
    precondition(!request.matches(itemId: "other", version: 7, sha256: digest, mime: "text/vcard"))
    precondition(!request.matches(itemId: "item", version: 8, sha256: digest, mime: "text/vcard"))
    precondition(!request.matches(itemId: "item", version: 7, sha256: "wrong", mime: "text/vcard"))
    precondition(!request.matches(itemId: "item", version: 7, sha256: digest, mime: "text/plain"))
    for invalid in ["{}", "{\"uri\":\"x\",\"itemId\":\"item\",\"version\":0,\"sha256\":\"\(digest)\"}", String(repeating: " ", count: 4097)] {
      denied { _ = try ZoenContactPreview.Request.decode(invalid) }
    }
    let root = URL.temporaryDirectory.appendingPathComponent("zoen-contact-test-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let url = root.appendingPathComponent("contact.vcf")
    try bytes.write(to: url)
    let read = try ZoenContactPreview.read(url, expectedHash: digest)
    precondition(read == contact)
    try Data("changed".utf8).write(to: url)
    denied { _ = try ZoenContactPreview.read(url, expectedHash: digest) }
    try Data(repeating: 65, count: ZoenContactPreview.maximumBytes + 1).write(to: url)
    denied { _ = try ZoenContactPreview.read(url, expectedHash: digest) }
    print("Contact parser PASS: actual Contacts vCard/folding/whitelist, malformed/multi/limits/hash, exact request binding and changed file rejection; no account/UI authority proof.")
  }
}
