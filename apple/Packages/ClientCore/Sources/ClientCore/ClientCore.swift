// Swift ergonomics on top of the generated bindings (ClientFFI.generated.swift).
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

public extension Client {
    /// Opens the account or demo store in the app container, excluded from system backups.
    static func openDefault(locale: String, demo: Bool) throws -> Client {
        let base = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            .appendingPathComponent(demo ? "demo" : "account", isDirectory: true)
        try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        var directory = base
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try directory.setResourceValues(values)
        return try Client.open(path: base.appendingPathComponent("client.sqlite").path, locale: locale)
    }
}
