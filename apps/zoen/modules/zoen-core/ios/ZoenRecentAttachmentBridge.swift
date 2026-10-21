import Foundation

/// Small source provenance only. Bytes/keys never enter the JS list or a new
/// durable store; the existing private URI owner holds the disposable copy.
enum ZoenRecentAttachmentBridge {
  struct Reference: Decodable {
    let spaceId: String
    let messageId: String
    let itemId: String
    let version: UInt32
    let sha256: String
    enum CodingKeys: String, CodingKey { case spaceId = "space_id", messageId = "message_id", itemId = "item_id", version, sha256 }
    var core: RecentAttachmentReference { RecentAttachmentReference(spaceId: spaceId, messageId: messageId, itemId: itemId, version: version, sha256: sha256) }
  }
  struct Request: Decodable {
    let scope: ZoenMediaDraftBridge.Scope
    let kind: String
    let reference: Reference
    var coreKind: RecentAttachmentKind {
      get throws {
        switch kind {
        case "file": return .file
        case "audio": return .audio
        default: throw ZoenMediaFiles.Failure.invalidFile
        }
      }
    }
  }
  static func decode(_ json: String) throws -> Request {
    guard json.utf8.count <= 16 << 10 else { throw ZoenMediaFiles.Failure.tooLarge }
    return try JSONDecoder().decode(Request.self, from: Data(json.utf8))
  }
}
