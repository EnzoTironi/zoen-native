import Foundation

struct LivingConversationSummary: Decodable, Equatable, Identifiable {
    let id: String
    let title: String
}

struct LivingHackathon: Decodable, Equatable, Identifiable {
    let id: String
    let title: String
    let deadlineMs: Int64
    let registered: [String]
    let profiles: [String: String]?
    let teams: [Team]
    let schedule: [Milestone]
    let winners: [String]
    let finalized: Bool
    let canJudge: Bool?
    let pendingTeamRequests: [TeamRequest]?

    var deadline: Date { Date(timeIntervalSince1970: Double(deadlineMs) / 1000) }
    enum CodingKeys: String, CodingKey {
        case id, title, registered, profiles, teams, schedule, winners, finalized
        case deadlineMs = "deadline_ms"
        case canJudge = "can_judge"
        case pendingTeamRequests = "pending_team_requests"
    }
    struct TeamRequest: Decodable, Equatable {
        let teamId: String
        let identityId: String
        enum CodingKeys: String, CodingKey { case teamId = "team_id"; case identityId = "identity_id" }
    }
    struct Team: Decodable, Equatable, Identifiable {
        let id: String
        let name: String
        let space: String?
        let members: [String]
        let projectUrl: String?
        let files: [String]
        let votes: Int
        let score: Int
        enum CodingKeys: String, CodingKey {
            case id, name, space, members, files, votes, score
            case projectUrl = "project_url"
        }
    }
    struct Milestone: Decodable, Equatable {
        let title: String
        let atMs: Int64
        var date: Date { Date(timeIntervalSince1970: Double(atMs) / 1000) }
        enum CodingKeys: String, CodingKey { case title; case atMs = "at_ms" }
    }
}
