// Swift ergonomics on top of the generated bindings (RodaFFI.generated.swift).
// No business rules here: the Rust core is the single source of truth. User-facing
// strings live in the app target (Localizable.xcstrings), not in this package.

import Foundation

extension Persona: Identifiable {}
extension SpaceSummary: Identifiable {}
extension TimelineEntry: Identifiable {}
extension AgentRequestDto: Identifiable {}
extension PlanLineDto: Identifiable {}
extension ItemDetail: Identifiable {}
extension AgentProfile: Identifiable { public var id: String { persona.id } }
extension PlanSectionDto: Identifiable { public var id: String { title } }
extension VersionDto: Identifiable { public var id: UInt32 { number } }
extension Mention: Identifiable { public var id: String { entry.id } }
extension LogReport: Identifiable { public var id: String { spaceId } }
extension LogEventDto: Identifiable { public var id: String { hash } }
extension DecisionPreview: Identifiable { public var id: String { action } }
extension AgentSpaceTrust: Identifiable { public var id: String { spaceId } }
extension SearchHit: Identifiable {
    public var id: String { "\(spaceId)|\(itemId ?? "-")|\(atMs)|\(snippet.hashValue)" }
}

public enum Money {
    /// Language tag the app runs in ("en" or "pt-BR"); set once at launch by the app.
    nonisolated(unsafe) public static var locale = "en"
    /// Same formatter as the core, so the app and Rust never disagree: "$1,348" or "R$ 1.348".
    public static func format(_ cents: Int64) -> String { formatMoney(cents: cents, locale: locale) }
}

extension TrustLevelDto: CaseIterable {
    public static var allCases: [TrustLevelDto] { [.listen, .suggest, .act, .autonomous] }

    public var symbol: String {
        switch self {
        case .listen: "ear"
        case .suggest: "text.bubble"
        case .act: "hand.tap"
        case .autonomous: "bolt.fill"
        }
    }
}

public extension RodaEngine {
    /// Opens the app database at Application Support/Roda/roda.sqlite, speaking `locale`
    /// (agent output, demo data and money follow it).
    static func openDefault(locale: String) throws -> RodaEngine {
        let base = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            .appendingPathComponent("Roda", isDirectory: true)
        try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        return try RodaEngine.open(path: base.appendingPathComponent("roda.sqlite").path, locale: locale)
    }
}
