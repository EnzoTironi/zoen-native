import Foundation
import RodaCore

/// The app's language. English is the development language; Portuguese (pt-BR) is the
/// second full translation (Localizable.xcstrings). The Rust core, the Foundation Models
/// prompts, money and demo data all follow the same choice, so nothing mixes languages.
enum AppLocale {
    /// What the String Catalog resolved for this launch ("en" or "pt-BR").
    static let isPortuguese: Bool = (Bundle.main.preferredLocalizations.first ?? "en").lowercased().hasPrefix("pt")
    /// BCP-47 tag handed to the core and to MCP Views.
    static var tag: String { isPortuguese ? "pt-BR" : "en" }
    /// Locale for numbers and dates that matches the strings on screen.
    static var locale: Locale { isPortuguese ? Locale(identifier: "pt_BR") : Locale(identifier: "en_US") }
    /// Language name for model instructions ("Answer in …").
    static var languageName: String { isPortuguese ? "Brazilian Portuguese" : "English" }
    /// ISO currency for the money text fields.
    static var currencyCode: String { isPortuguese ? "BRL" : "USD" }
}

extension TrustLevelDto {
    var label: String {
        switch self {
        case .listen: String(localized: "Listen")
        case .suggest: String(localized: "Suggest")
        case .act: String(localized: "Act")
        case .autonomous: String(localized: "Autonomous")
        }
    }

    var explanation: String {
        switch self {
        case .listen: String(localized: "Reads only when called and answers with text.")
        case .suggest: String(localized: "Prepares drafts and proposals; nothing goes out without you.")
        case .act: String(localized: "Does what’s reversible, within budget, always with undo.")
        case .autonomous: String(localized: "Also acts outside (calendar, messages, small payments) within limits.")
        }
    }
}

extension CoreError {
    /// Message ready for the UI (the core already speaks the app's language).
    var message: String {
        switch self {
        case .Storage(let m): String(localized: "Storage error: \(m)")
        case .NotFound(let what): String(localized: "Not found: \(what)")
        case .Forbidden(let r), .Invalid(let r), .Stale(let r): r
        }
    }
}

/// Titles of the demo Spaces as the core seeds them (in the app's language). Data, not UI.
enum DemoSpace {
    static var paraty: String { AppLocale.isPortuguese ? "Paraty com a Marina" : "Paraty with Marina" }
    static var saturdayCrew: String { AppLocale.isPortuguese ? "Turma do Sábado" : "Saturday Crew" }
    static var coastalTravelers: String { AppLocale.isPortuguese ? "Viajantes do Litoral" : "Coastal Travelers" }
}

extension AppLocale {
    /// Demo content (simulated chat lines, story prompts) in the app's language. Not UI chrome,
    /// so it doesn't go through the String Catalog.
    static func pick(_ pt: String, _ en: String) -> String { isPortuguese ? pt : en }
}
