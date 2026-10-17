import Foundation

struct LivingTierInvite: Identifiable {
    let id = UUID()
    let space: String
    let token: String

    init?(url: URL) {
        guard url.scheme == "zoen", url.host == "tier",
              let query = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems,
              let space = query.first(where: { $0.name == "space" })?.value, !space.isEmpty,
              let token = query.first(where: { $0.name == "token" })?.value, !token.isEmpty else { return nil }
        self.space = space
        self.token = token
    }
}

struct LivingTier: Decodable, Equatable, Identifiable {
    let id: String
    let name: String
    let space: String?
    let groups: [String]
    let resources: [Resource]
    let members: [String]
    struct Resource: Decodable, Equatable, Identifiable { let id: String; let kind: String }
}

struct LivingInvitation: Decodable, Equatable, Identifiable {
    let id: String
    let tierId: String
    let expiresAtMs: Int64
    let revoked: Bool
    var expiry: Date { Date(timeIntervalSince1970: Double(expiresAtMs) / 1000) }
    enum CodingKeys: String, CodingKey {
        case id, revoked
        case tierId = "tier_id"
        case expiresAtMs = "expires_at_ms"
    }
}

struct LivingTierEntitlement: Decodable, Equatable {
    let identityId: String
    let tierId: String
    let source: String
    enum CodingKeys: String, CodingKey {
        case source
        case identityId = "identity_id"
        case tierId = "tier_id"
    }
}

struct LivingTierClaim: Decodable, Equatable {
    let identityId: String
    let inviteId: String
    let tierId: String
    enum CodingKeys: String, CodingKey {
        case identityId = "identity_id"
        case inviteId = "invite_id"
        case tierId = "tier_id"
    }
}
