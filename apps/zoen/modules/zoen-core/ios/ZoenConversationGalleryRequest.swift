import Foundation

/// Candidates are navigation hints. Only the canonical signed message projection
/// can admit a page; a caller cannot use an arbitrary list as authorization.
struct ZoenConversationGalleryRequest: Decodable {
  struct Reference: Decodable, Equatable, Sendable {
    let messageId: String
    let itemId: String
    let version: UInt32
  }
  let spaceId: String
  let selectedId: String
  let entries: [Reference]

  static func decode(_ json: String) throws -> Self {
    guard json.utf8.count <= 16384 else { throw ZoenFilePreview.Failure.changed }
    let value = try JSONDecoder().decode(Self.self, from: Data(json.utf8))
    let ids = [value.spaceId, value.selectedId] + value.entries.flatMap { [$0.messageId, $0.itemId] }
    guard ids.allSatisfy({ !$0.isEmpty && $0.utf8.count <= 256 && !$0.contains("\0") }),
          (1...10).contains(value.entries.count),
          Set(value.entries.map(\.messageId)).count == value.entries.count,
          value.entries.allSatisfy({ $0.version > 0 }),
          value.entries.contains(where: { $0.messageId == value.selectedId }) else { throw ZoenFilePreview.Failure.changed }
    return value
  }

  struct Message: Equatable, Sendable {
    let reference: Reference
    let authorId: String
    let author: String
    let caption: String
    let thread: String?
    let reply: String?
    let groupId: String?
    let index: Int?
    let count: Int?

    init(json: String, expected: Reference) throws {
      guard let value = try JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any],
            value["id"] as? String == expected.messageId,
            value["deleted"] as? Bool == false,
            value["forwarded"] as? Bool == false,
            value["voice"] == nil || value["voice"] is NSNull,
            let author = value["author"] as? [String: Any], let authorId = author["id"] as? String,
            let kind = value["kind"] as? [String: Any], let message = kind["Message"] as? [String: Any],
            let card = message["card"] as? [String: Any], card["kind_id"] as? String == "file",
            card["item_id"] as? String == expected.itemId,
            let version = card["version"] as? NSNumber, version.doubleValue == Double(expected.version),
            let text = message["text"] as? String, text.utf8.count <= 65536 else { throw ZoenFilePreview.Failure.changed }
      let media = value["media"] as? [String: Any]
      guard media?["presentation"] as? String != "file" else { throw ZoenFilePreview.Failure.unavailable }
      let group = media?["group"] as? [String: Any]
      reference = expected; self.authorId = authorId
      self.author = String((author["name"] as? String ?? "").prefix(255))
      caption = text
      thread = value["in_thread"] as? String
      reply = (value["reply_to"] as? [String: Any])?["id"] as? String
      groupId = group?["id"] as? String
      index = group?["index"] as? Int; count = group?["count"] as? Int
    }
  }

  /// Reconstruct ordering from the signed index, not the request array, filename
  /// or timestamp. Partial/conflicting membership never creates a pager.
  func reconstruct(_ messages: [Message]) throws -> [Message] {
    guard messages.count == entries.count, Set(messages.map(\.reference.messageId)) == Set(entries.map(\.messageId)),
          let selected = messages.first(where: { $0.reference.messageId == selectedId }) else { throw ZoenFilePreview.Failure.changed }
    if messages.count == 1 { return messages }
    guard let group = selected.groupId, group.hasPrefix("album_"), group.count == 70,
          group.dropFirst(6).allSatisfy({ "0123456789abcdef".contains($0) }),
          selected.count == messages.count,
          Set(messages.compactMap(\.index)) == Set(0..<messages.count),
          messages.allSatisfy({
            $0.groupId == group && $0.count == messages.count && $0.authorId == selected.authorId &&
            $0.thread == selected.thread && $0.reply == selected.reply
          }) else { throw ZoenFilePreview.Failure.changed }
    return messages.sorted { $0.index! < $1.index! }
  }
}

/// A read-only presentation may end because the app backgrounds or its source
/// retires. The operation must await FilePreview's dismissal and verified cleanup
/// first; any real cleanup/presentation error still rejects the bridge Promise.
enum ZoenConversationGalleryCompletion {
  static func awaitDismissal(_ operation: () async throws -> Void) async throws {
    do { try await operation() }
    catch is CancellationError { }
  }
}

/// Transient CAS writer for the Rust-owned checkpoint. Never owns a file or grants playback access.
@MainActor final class ZoenMediaPlaybackBookmark {
  struct Position: Codable, Equatable, Sendable {
    let position_ms: UInt32
    let duration_ms: UInt32
    let rate_milli: UInt32
  }
  struct Value: Codable, Sendable { let revision: String; let position: Position? }
  private let save: @Sendable (String, Position) async throws -> Value
  private var revision: String
  private var last: Position?
  private var pending: Position?
  private var task: Task<Void, Never>?
  private(set) var failed = false
  private(set) var position: Position?
  var resumeSeconds: Double? {
    position.map { $0.position_ms >= $0.duration_ms ? 0 : Double($0.position_ms) / 1000 }
  }
  init(value: Value, save: @escaping @Sendable (String, Position) async throws -> Value) {
    revision = value.revision; position = value.position; last = value.position; self.save = save
  }
  func observe(seconds: Double, duration: Double, playing: Bool) {
    guard !failed, seconds.isFinite, duration.isFinite, duration > 0, duration <= 604800 else { return }
    let total = UInt32((duration * 1000).rounded())
    guard total > 0 else { return }
    let next = Position(position_ms: UInt32((max(0, min(duration, seconds)) * 1000).rounded()), duration_ms: total, rate_milli: 1000)
    position = next
    let previous = pending ?? last
    guard previous != next else { return }
    if playing, let previous, previous.position_ms / 5000 == next.position_ms / 5000 { return }
    pending = next
    guard task == nil else { return }
    task = Task { [self] in
      defer { task = nil }
      while let next = pending, !failed {
        pending = nil
        do { let result = try await save(revision, next); revision = result.revision; last = result.position }
        catch { failed = true; pending = nil } // Reopen must read again; uncertain/stale writes are never replayed.
      }
    }
  }
  func flush() async { await task?.value }
}
