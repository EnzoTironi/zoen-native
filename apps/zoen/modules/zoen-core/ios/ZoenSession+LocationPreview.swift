import Foundation

extension ZoenSession {
  func readLocationFile(_ json: String) throws -> String {
    let request = try ZoenStaticLocation.Request.decode(json)
    let url = try mediaFiles.ownedSelectionURL(request.uri)
    guard !erasing, !closing, let binding = previewBindings[url.path], bindingIsCurrent(binding),
          request.matches(itemId: binding.itemId, version: binding.version, sha256: binding.sha256, mime: binding.mime) else {
      throw SessionError.closed
    }
    // Exactly the same held-file/Core permission boundary as the contact reader.
    // The disposable copy is never sufficient authority after source revocation.
    let before = try binding.engine.fileInfoAt(itemId: binding.itemId, version: binding.version)
    guard before.sha256 == binding.sha256, before.mime == binding.mime,
          before.bytes <= UInt64(ZoenStaticLocation.maximumBytes) else { throw ZoenStaticLocation.Failure.invalid }
    let point = try ZoenStaticLocation.read(url, expectedHash: binding.sha256)
    let after = try binding.engine.fileInfoAt(itemId: binding.itemId, version: binding.version)
    guard !erasing, !closing, bindingIsCurrent(binding),
          try mediaFiles.ownedSelectionURL(request.uri) == url,
          after.sha256 == before.sha256, after.mime == before.mime, after.bytes == before.bytes else {
      throw SessionError.closed
    }
    return String(decoding: try JSONEncoder().encode(point), as: UTF8.self)
  }
}
