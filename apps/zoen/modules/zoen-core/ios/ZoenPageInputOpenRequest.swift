import Foundation

struct ZoenPageInputOpenRequest: Decodable, Equatable, Sendable {
  let session: String
  let id: String
  let itemId: String
  let blockId: String?
  let resume: Bool
  let revision: String
  let editContext: String
  let mode: String
  enum Target: String { case document, append, block }
  let target: Target

  private enum CodingKeys: String, CodingKey {
    case session, id, itemId, blockId, resume, revision, editContext, mode, target
  }
  init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    session = try c.decode(String.self, forKey: .session)
    id = try c.decode(String.self, forKey: .id)
    itemId = try c.decode(String.self, forKey: .itemId)
    blockId = try c.decodeIfPresent(String.self, forKey: .blockId)
    resume = try c.decode(Bool.self, forKey: .resume)
    revision = try c.decode(String.self, forKey: .revision)
    editContext = try c.decode(String.self, forKey: .editContext)
    mode = try c.decode(String.self, forKey: .mode)
    if c.contains(.target) {
      guard let explicit = Target(rawValue: try c.decode(String.self, forKey: .target)) else {
        throw ZoenPageInputLease.Failure.unsupported
      }
      target = explicit
    } else {
      // Legacy nil means append, never whole-document replacement.
      target = blockId == nil ? .append : .block
    }
    switch target {
    case .document:
      guard blockId == nil, mode == "markdown" else { throw ZoenPageInputLease.Failure.unsupported }
    case .append:
      guard blockId == nil else { throw ZoenPageInputLease.Failure.unsupported }
    case .block:
      guard blockId != nil else { throw ZoenPageInputLease.Failure.unsupported }
    }
  }

  static func decode(_ json: String) throws -> Self {
    guard json.utf8.count <= 256 * 1024 else { throw ZoenPageInputLease.Failure.invalidCapture }
    let data = Data(json.utf8)
    guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
          Set(object.keys).isSubset(of: ["session", "id", "itemId", "blockId", "resume", "revision", "editContext", "mode", "target"]) else {
      throw ZoenPageInputLease.Failure.invalidCapture
    }
    let value = try JSONDecoder().decode(Self.self, from: data)
    guard !value.itemId.isEmpty, value.itemId.utf8.count <= 256,
          value.blockId.map({ !$0.isEmpty && $0.utf8.count <= 256 }) ?? true,
          ["rich", "markdown", "lossless-text"].contains(value.mode),
          value.editContext.utf8.count <= 128 * 1024,
          let revision = UInt64(value.revision), String(revision) == value.revision else {
      throw ZoenPageInputLease.Failure.unsupported
    }
    return value
  }

  static func seedBlock(_ json: String) throws -> [String: Any]? {
    let value = try JSONSerialization.jsonObject(with: Data(json.utf8), options: [.fragmentsAllowed])
    if value is NSNull { return nil }
    guard let tagged = value as? [String: Any], tagged.count == 1,
          let block = tagged["Page"] as? [String: Any] else { throw ZoenPageInputLease.Failure.unsupported }
    return block
  }
}

