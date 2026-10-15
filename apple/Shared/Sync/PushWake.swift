import Foundation
import CoreFoundation

/// A content-free wake from the relay (ADR 0047). The payload never carries message
/// content: the app syncs, decrypts on the device and decides whether to notify.
struct PushWake: Equatable, Sendable {
    /// Optional untrusted sync hint. Empty means "catch up on everything".
    let spaceId: String
    let eventSequence: UInt64

    static let catchUp = PushWake(spaceId: "", eventSequence: 0)

    private init(spaceId: String, eventSequence: UInt64) {
        self.spaceId = spaceId
        self.eventSequence = eventSequence
    }

    var isCatchUp: Bool { spaceId.isEmpty }

    static func decode(_ payload: Data) -> PushWake? {
        guard payload.count <= 4096,
              let object = try? JSONSerialization.jsonObject(with: payload),
              let fields = object as? [AnyHashable: Any] else { return nil }
        return decode(fields)
    }

    static func decode(_ fields: [AnyHashable: Any]) -> PushWake? {
        // What the relay sends: exactly {"aps":{"content-available":1}}.
        if fields.count == 1, let aps = fields["aps"] as? [String: Any],
           aps.count == 1, oneInteger(aps["content-available"]) {
            return .catchUp
        }
        // Optional hint (never content): aps + zoen.{v,space_id,event_seq}
        guard fields.count == 2,
              let aps = fields["aps"] as? [String: Any], aps.count == 1,
              oneInteger(aps["content-available"]),
              let zoen = fields["zoen"] as? [String: Any], zoen.count == 3,
              oneInteger(zoen["v"]),
              let space = zoen["space_id"] as? String, validSpace(space),
              let cursor = zoen["event_seq"] as? String,
              !cursor.isEmpty, cursor.utf8.count <= 20, cursor.utf8.first != 48,
              cursor.utf8.allSatisfy({ (48...57).contains($0) }),
              let sequence = UInt64(cursor), sequence > 0 else { return nil }
        return PushWake(spaceId: space, eventSequence: sequence)
    }

    private static func oneInteger(_ value: Any?) -> Bool {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID(),
              ["c", "s", "i", "l", "q", "C", "S", "I", "L", "Q"].contains(String(cString: number.objCType)) else { return false }
        return number.uint64Value == 1
    }

    private static func validSpace(_ value: String) -> Bool {
        !value.isEmpty && value.utf8.count <= 128 && value.utf8.allSatisfy {
            (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) || $0 == 45 || $0 == 95
        }
    }
}

enum PushEnvironment: String, Equatable, Sendable {
    case sandbox, production

    init?(apsEntitlement: String) {
        switch apsEntitlement {
        case "development": self = .sandbox
        case "production": self = .production
        default: return nil
        }
    }

    /// Wire platform string the relay expects.
    var platform: String {
        switch self {
        case .sandbox: "apns-sandbox"
        case .production: "apns"
        }
    }
}

struct PushConfiguration: Equatable, Sendable {
    let environment: PushEnvironment
    let topic: String

    init?(apsEntitlement: String?, topic: String?) {
        guard let apsEntitlement, let environment = PushEnvironment(apsEntitlement: apsEntitlement),
              let topic, !topic.isEmpty, topic.utf8.count <= 255,
              topic.contains("."), !topic.split(separator: ".", omittingEmptySubsequences: false).contains(where: { $0.isEmpty }),
              topic.utf8.allSatisfy({
                  (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) || $0 == 45 || $0 == 46
              }) else { return nil }
        self.environment = environment
        self.topic = topic
    }

    static func tokenHex(_ bytes: Data) -> String? {
        guard !bytes.isEmpty, bytes.count <= 256 else { return nil }
        return bytes.map { String(format: "%02x", $0) }.joined()
    }
}
