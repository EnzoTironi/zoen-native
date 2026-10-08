import Foundation
import RodaCore

/// Reads the chat after a hike was proposed and works out the day: who drives (the person
/// who offered a car) and who rides (whoever said they're in, plus everyone who voted).
/// Deterministic rules on purpose: it's the same on every device and easy to audit.
enum HikePlanner {
    private static let driverCues = ["i can drive", "i'll drive", "i’ll drive", "i have a car", "i've got the car", "i’ve got the car", "my car", "room for",
                                     "eu dirijo", "tenho carro", "levo o carro", "vou de carro", "posso dirigir", "cabem mais"]
    private static let inCues = ["i'm in", "i’m in", "im in", "count me in", "in!", "i'm down", "i’m down", "down!",
                                 "to dentro", "eu vou", "bora", "conta comigo"]

    /// Pickup neighbourhoods come from the demo profiles (there's no address book here).
    static let demoNeighborhoods: [String: String] = ["Marina": "Hayes Valley", "Ana": "Mission", "Enzo": "Noe Valley", "Lucas": "Inner Sunset"]
    /// The order a car from the Sunset would pick people up in.
    private static let route = ["Inner Sunset", "Noe Valley", "Mission", "Hayes Valley"]

    static func plan(entries: [TimelineEntry], since: Int64, voters: [String]) -> String? {
        var driver: String?
        var riders: [String] = []
        for e in entries where e.atMs >= since && e.author.kind == .person {
            guard case .message(let text, _) = e.kind else { continue }
            let t = " " + text.lowercased().folding(options: .diacriticInsensitive, locale: nil) + " "
            if driver == nil, driverCues.contains(where: { t.contains($0.folding(options: .diacriticInsensitive, locale: nil)) }) { driver = e.author.name }
            if inCues.contains(where: { t.contains($0.folding(options: .diacriticInsensitive, locale: nil)) }) { riders.append(e.author.name) }
        }
        guard let driver else { return nil }
        var seen: Set<String> = [driver]
        let people = (riders + voters).filter { seen.insert($0).inserted }
        guard !people.isEmpty else { return nil }
        let start = demoNeighborhoods[driver]
        var byPlace: [String: [String]] = [:]
        for p in people { byPlace[demoNeighborhoods[p] ?? "", default: []].append(p) }
        let order = byPlace.keys.sorted { (route.firstIndex(of: $0) ?? 99, $0) < (route.firstIndex(of: $1) ?? 99, $1) }
        let pickups: [[String: Any]] = order.compactMap { place in
            // Someone who lives where the car starts is already in it.
            guard place != start else { return nil }
            return ["names": byPlace[place] ?? [], "place": place]
        }
        var args: [String: Any] = ["driver": driver, "pickups": pickups]
        if let start, let same = byPlace[start], !same.isEmpty {
            args["pickups"] = [["names": same, "place": start]] + pickups
        }
        guard let data = try? JSONSerialization.data(withJSONObject: args) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}
