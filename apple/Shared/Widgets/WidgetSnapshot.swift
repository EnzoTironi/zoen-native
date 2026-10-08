import Foundation

/// A mini-app's live snapshot in one of a few native templates. WidgetKit can't run a
/// mini-app's HTML, so every mini-app can publish one of these; the same renderer draws it
/// in the Home strip and (later) in the widget extension.
///
/// Rules (enforced by `validated()`): known template, short strings, at most 3 bars and 4
/// list rows, a 6-digit accent, an SF Symbol name, a `zoen://app/<id>` deep link, and no
/// URLs anywhere (images are only the app's own bundled art, picked by `art`).
struct WidgetSnapshot: Codable, Hashable, Identifiable, Sendable {
    enum Template: String, Codable, Sendable { case stat, progress, countdown, list, caption, ticket, photo }

    struct Bar: Codable, Hashable, Sendable { var label: String; var value: Double }
    struct Row: Codable, Hashable, Sendable { var text: String; var done: Bool }
    /// An interactive widget button: a reversible `app` tool of the mini-app (Feed, Nap).
    struct Action: Codable, Hashable, Sendable { var tool: String; var label: String }

    var id: String
    var appId: String
    var template: Template
    var title: String
    var eyebrow: String? = nil
    var value: String? = nil
    var detail: String? = nil
    var bars: [Bar]? = nil
    var rows: [Row]? = nil
    /// Countdown target (ms since 1970).
    var targetMs: Int64? = nil
    var accentHex: String
    var symbol: String
    /// Which bundled art to draw ("pet", "pet.asleep", "globe", "pot", or nil).
    var art: String? = nil
    /// Hide on a locked lock screen.
    var sensitive: Bool? = nil
    var deepLink: String
    var actions: [Action]? = nil
    /// `ticket`: two short codes ("SF", "TML"), their places and two times (leave, arrive).
    var codes: [String]? = nil
    var places: [String]? = nil
    var times: [String]? = nil
    /// `photo`: one of the app's bundled photos; with `targetMs` it carries a glass countdown.
    var photo: String? = nil

    static let photos: Set<String> = ["hike-tomales", "hike-steep", "hike-lands"]

    /// Decodes and validates a snapshot from the core (`AppStateDto.snapshotJson`).
    static func decode(_ json: String) -> WidgetSnapshot? {
        guard !json.isEmpty, let data = json.data(using: .utf8) else { return nil }
        return (try? JSONDecoder().decode(WidgetSnapshot.self, from: data))?.validated()
    }

    /// Whole days and hours left for a countdown.
    func remaining(at now: Date = .now) -> (days: Int, hours: Int, minutes: Int) {
        let left = max(0, Double(targetMs ?? 0) / 1000 - now.timeIntervalSince1970)
        let m = Int(left / 60)
        return (m / 1440, (m % 1440) / 60, m % 60)
    }

    static let maxText = 48

    func validated() -> WidgetSnapshot? {
        func clean(_ s: String?) -> String? {
            guard let s else { return nil }
            if s.contains("://") { return nil }
            return String(s.prefix(Self.maxText))
        }
        guard accentHex.range(of: "^#[0-9A-Fa-f]{6}$", options: .regularExpression) != nil,
              symbol.range(of: "^[a-z0-9.]{1,40}$", options: .regularExpression) != nil,
              deepLink == "zoen://app/\(id)",
              let t = clean(title), !t.isEmpty else { return nil }
        var s = self
        s.title = t
        s.eyebrow = clean(eyebrow); s.value = clean(value); s.detail = clean(detail)
        s.bars = bars.map { Array($0.prefix(3)).map { Bar(label: String($0.label.prefix(16)), value: min(1, max(0, $0.value))) } }
        s.rows = rows.map { Array($0.prefix(4)).compactMap { r in clean(r.text).map { Row(text: $0, done: r.done) } } }
        s.actions = actions.map { Array($0.prefix(2)).filter { $0.tool.range(of: "^[a-z_]{1,32}$", options: .regularExpression) != nil }.map { Action(tool: $0.tool, label: String($0.label.prefix(14))) } }
        s.codes = codes.map { Array($0.prefix(2)).map { String($0.prefix(5)) } }
        s.places = places.map { Array($0.prefix(2)).compactMap { clean($0).map { String($0.prefix(20)) } } }
        s.times = times.map { Array($0.prefix(2)).map { String($0.prefix(8)) } }
        if let photo, !Self.photos.contains(photo) { s.photo = nil }
        if template == .photo && s.photo == nil { return nil }
        if template == .ticket && (s.codes?.count ?? 0) < 2 { return nil }
        if let art, !["pet", "pet.asleep", "pet.gone", "globe", "pot", "ballot", "notepad", "trip", "hike"].contains(art) { s.art = nil }
        return s
    }
}
