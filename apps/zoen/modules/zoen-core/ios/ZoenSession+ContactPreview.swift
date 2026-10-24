import Foundation

extension ZoenSession {
  func readContactFile(_ json: String) throws -> String {
    let request = try ZoenContactPreview.Request.decode(json)
    let url = try mediaFiles.ownedSelectionURL(request.uri)
    guard !erasing, !closing, let binding = previewBindings[url.path], bindingIsCurrent(binding),
          request.matches(itemId: binding.itemId, version: binding.version, sha256: binding.sha256, mime: binding.mime) else {
      throw SessionError.closed
    }
    // The current encrypted-file read gate remains authoritative even if the
    // disposable copy still exists after removal/revocation of its source.
    let before = try binding.engine.fileInfoAt(itemId: binding.itemId, version: binding.version)
    guard before.sha256 == binding.sha256, before.mime == binding.mime,
          before.bytes <= UInt64(ZoenContactPreview.maximumBytes) else { throw ZoenContactPreview.Failure.invalid }
    let contact = try ZoenContactPreview.read(url, expectedHash: binding.sha256)
    let after = try binding.engine.fileInfoAt(itemId: binding.itemId, version: binding.version)
    guard !erasing, !closing, bindingIsCurrent(binding),
          try mediaFiles.ownedSelectionURL(request.uri) == url,
          after.sha256 == before.sha256, after.mime == before.mime, after.bytes == before.bytes else {
      throw SessionError.closed
    }
    return String(decoding: try JSONEncoder().encode(contact), as: UTF8.self)
  }
}
