import Foundation

@main
struct LivingSpaceProjectionJourney {
    static func main() throws {
        let home = try LivingSpaceProjection.decode("""
        {"space":"s","page":"p","announcement_items":["a"],"can_manage":true,"pending_requests":["person"],"cover_file":"cover","file_items":["cover"],"event_items":["schedule"]}
        """)
        precondition(home.space == "s" && home.page == "p")
        precondition(home.announcementItems == ["a"] && home.pendingRequests == ["person"])
        precondition(home.canManage)
        precondition(home.coverFile == "cover" && home.fileItems == ["cover"] && home.eventItems == ["schedule"])
        let empty = try LivingSpaceProjection.decode("""
        {"space":"s","page":null,"announcement_items":[],"can_manage":false,"pending_requests":[]}
        """)
        precondition(empty.page == nil && !empty.canManage && empty.pendingRequests.isEmpty)
        do {
            _ = try LivingSpaceProjection.decode("{\"space\":\"s\"}")
            preconditionFailure("Incomplete state must not imply approval permissions")
        } catch { }
        let expanded = try LivingSpaceProjection.decode("""
        {"space":"s","page":null,"announcement_items":[],"can_manage":true,"pending_requests":[],"hackathon":{"id":"h","title":"Build together","deadline_ms":1000,"registered":["me"],"teams":[],"schedule":[],"winners":[],"finalized":false},"tiers":[{"id":"t","name":"Builder","space":"encrypted","groups":[],"resources":[],"members":["me"]}],"entitlements":[{"identity_id":"me","tier_id":"t","source":"manual"}],"tier_claims":[{"identity_id":"other","invite_id":"invite","tier_id":"t"}]}
        """)
        precondition(expanded.hackathon?.deadline == Date(timeIntervalSince1970: 1))
        precondition(expanded.tiers?.first?.space == "encrypted")
        precondition(expanded.entitlements?.first?.source == "manual" && expanded.tierClaims?.first?.identityId == "other")
        precondition(LivingTierInvite(url: URL(string: "zoen://tier?space=s&token=sample")!)?.space == "s")
        precondition(LivingTierInvite(url: URL(string: "https://example.com/tier?space=s&token=sample")!) == nil)
        precondition(LivingTierInvite(url: URL(string: "zoen://tier?token=sample")!) == nil)
        let hiddenTier = try JSONDecoder().decode(LivingTier.self, from: Data("{\"id\":\"locked\",\"name\":\"Builder\",\"space\":null,\"groups\":[],\"resources\":[],\"members\":[]}".utf8))
        precondition(hiddenTier.space == nil && hiddenTier.groups.isEmpty)
        let hiddenTeam = try JSONDecoder().decode(LivingHackathon.Team.self, from: Data("{\"id\":\"team\",\"name\":\"Builders\",\"space\":null,\"members\":[],\"project_url\":null,\"files\":[],\"votes\":0,\"score\":0}".utf8))
        precondition(hiddenTeam.space == nil)
        print("Living Space projection: 9 cases passed")
    }
}
