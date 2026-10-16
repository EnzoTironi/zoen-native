import Foundation

/// The small experiment envelope; content and page permissions remain in Core.
struct LivingSpaceProjection: Decodable, Equatable {
    let space: String
    let page: String?
    let announcementItems: [String]
    let canManage: Bool
    let pendingRequests: [String]
    let coverFile: String?
    let fileItems: [String]?
    let eventItems: [String]?
    let hackathon: LivingHackathon?
    let conversations: [LivingConversationSummary]?
    let conversationSpace: String?
    let conversationMemberCap: Int?
    let tiers: [LivingTier]?
    let entitlements: [LivingTierEntitlement]?
    let tierClaims: [LivingTierClaim]?
    let invitations: [LivingInvitation]?

    enum CodingKeys: String, CodingKey {
        case space, page, hackathon, conversations, tiers, entitlements, invitations
        case conversationSpace = "conversation_space"
        case conversationMemberCap = "conversation_member_cap"
        case tierClaims = "tier_claims"
        case announcementItems = "announcement_items"
        case canManage = "can_manage"
        case pendingRequests = "pending_requests"
        case coverFile = "cover_file"
        case fileItems = "file_items"
        case eventItems = "event_items"
    }

    static func decode(_ json: String) throws -> Self {
        try JSONDecoder().decode(Self.self, from: Data(json.utf8))
    }
}
