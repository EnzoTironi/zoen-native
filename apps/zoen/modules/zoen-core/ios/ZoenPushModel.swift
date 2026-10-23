import Foundation
import CoreFoundation

enum ZoenPushBuildAdmission {
  static func enabled(_ info: [String: Any], bundleIdentifier: String?) -> Bool {
    guard let bundleIdentifier, !bundleIdentifier.isEmpty,
          info["ZoenRemoteNotificationsEnabled"] as? Bool == true,
          ["sandbox", "production"].contains(info["ZoenPushEnvironment"] as? String ?? ""),
          info["ZoenPushTopic"] as? String == bundleIdentifier else { return false }
    return true
  }
}

/// Untrusted remote bytes can ask for a sync, never choose an account or route.
enum ZoenPushWakePayload {
  static func accepts(_ input: [AnyHashable: Any]) -> Bool {
    guard JSONSerialization.isValidJSONObject(input),
          let data = try? JSONSerialization.data(withJSONObject: input), data.count <= 4096,
          let aps = input["aps"] as? [String: Any], Set(aps.keys) == ["content-available"],
          integerOne(aps["content-available"]) else { return false }
    if Set(input.keys) == [AnyHashable("aps")] { return true }
    guard Set(input.keys) == [AnyHashable("aps"), AnyHashable("zoen")],
          let hint = input["zoen"] as? [String: Any],
          Set(hint.keys) == ["v", "space_id", "event_seq"], integerOne(hint["v"]),
          let space = hint["space_id"] as? String,
          space.utf8.count >= 1, space.utf8.count <= 128,
          space.utf8.allSatisfy({ (65...90).contains($0) || (97...122).contains($0) || (48...57).contains($0) || $0 == 95 || $0 == 45 }),
          let sequence = hint["event_seq"] as? String, exactSequence(sequence) != nil else { return false }
    return true
  }

  private static func integerOne(_ value: Any?) -> Bool {
    guard let number = value as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID(),
          !["f", "d"].contains(String(cString: number.objCType)) else { return false }
    return number.stringValue == "1"
  }
  static func exactSequence(_ value: String) -> UInt64? {
    guard (1...20).contains(value.utf8.count), value.first != "0",
          value.utf8.allSatisfy({ (48...57).contains($0) }), let parsed = UInt64(value), parsed > 0 else { return nil }
    return parsed
  }
}

struct ZoenPushNote: Equatable, Sendable {
  let identifier: String
  let title: String
  let body: String
  let spaceID: String
  let deviceID: String
  let sequence: String
}

/// Presentation-only projection of the same authenticated snapshot used by local.ts.
/// No push payload, owner keys, provider tokens or transport cursor enters this model.
struct ZoenPushSnapshot: Decodable, Sendable {
  struct Account: Decodable, Sendable {
    let identity_id: String
    let device_id: String
    let unlocked: Bool
    let setup_pending: Bool
  }
  struct Connection: Decodable, Sendable { let synced: Bool }
  struct Member: Decodable, Sendable { let id: String }
  struct Author: Decodable, Sendable { let name: String; let is_me: Bool }
  struct Conversation: Decodable, Sendable {
    let id: String
    let kind: String
    let title: String
    let muted: Bool
    let archived: Bool
    let unread: UInt64
    let event_count: String
    let last_author: Author?
    let last_preview: String
    let members: [Member]
  }
  let account: Account?
  let connection: Connection
  let conversations: [Conversation]

  init(json: String) throws { self = try JSONDecoder().decode(Self.self, from: Data(json.utf8)) }

  func notes(since before: ZoenPushSnapshot) -> [ZoenPushNote] {
    guard let account, account.unlocked, !account.setup_pending, connection.synced,
          before.account?.identity_id == account.identity_id,
          before.account?.device_id == account.device_id else { return [] }
    let previous = Dictionary(before.conversations.map { ($0.id, $0.unread) }, uniquingKeysWith: { _, new in new })
    return conversations.compactMap { chat in
      guard !chat.muted, !chat.archived, chat.last_author?.is_me != true,
            chat.unread > (previous[chat.id] ?? 0),
            chat.members.contains(where: { $0.id == account.identity_id }),
            ZoenPushWakePayload.exactSequence(chat.event_count) != nil else { return nil }
      let title = chat.kind == "Direct" ? (chat.last_author?.name ?? chat.title) : chat.title
      let body = chat.kind == "Direct" || chat.last_author == nil
        ? chat.last_preview : "\(chat.last_author!.name): \(chat.last_preview)"
      return ZoenPushNote(identifier: "zoen-local-\(account.device_id)-\(chat.id)-\(chat.event_count)",
        title: title, body: body, spaceID: chat.id, deviceID: account.device_id, sequence: chat.event_count)
    }
  }
}
