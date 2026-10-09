import SwiftUI
import ImageIO
#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

/// Zoen AvatarV1: stills in the catalog, hero/loop WebPs as folder refs, Manifest for tints/tags.
enum HandDrawnAvatarAsset {
    struct Asset: Identifiable, Hashable {
        let id: String
        let kind: Kind
        let name: String
        let tags: [String]
        let tint: String
        let backdrop: String
        let still: String
        let hero: String
        let loop: String
        enum Kind: String { case agent, group }
    }

    private static let catalog: [Asset] = {
        guard let url = Bundle.main.url(forResource: "AvatarV1Manifest", withExtension: "json")
                ?? Bundle.main.url(forResource: "Manifest", withExtension: "json", subdirectory: "AvatarV1"),
              let data = try? Data(contentsOf: url),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let items = root["assets"] as? [[String: Any]]
        else { return [] }
        return items.compactMap { row in
            guard let id = row["id"] as? String,
                  let kindRaw = row["kind"] as? String,
                  let kind = Asset.Kind(rawValue: kindRaw),
                  let name = row["name"] as? String,
                  let files = row["files"] as? [String: Any],
                  let still = files["still"] as? String
            else { return nil }
            let colors = row["colors"] as? [String: Any] ?? [:]
            return Asset(
                id: id,
                kind: kind,
                name: name,
                tags: (row["tags"] as? [String]) ?? [],
                tint: (colors["tint"] as? String) ?? "#888888",
                backdrop: (colors["backdrop"] as? String) ?? "#E8E0D4",
                still: still,
                hero: (files["hero"] as? String) ?? "Heroes/\(name).webp",
                loop: (files["loop"] as? String) ?? "Loops/\(name).webp"
            )
        }
    }()

    static var agents: [Asset] { catalog.filter { $0.kind == .agent } }
    static var groups: [Asset] { catalog.filter { $0.kind == .group } }
    static func asset(named name: String) -> Asset? {
        catalog.first { $0.name == name || $0.id == name || $0.still == name }
    }

    // MARK: Still (lists / Reduce Motion)

    static func still(named name: String) -> Image? {
        guard let a = asset(named: name) ?? catalog.first(where: { $0.still == name }) else {
            #if canImport(UIKit)
            if UIImage(named: name) != nil { return Image(name) }
            #elseif canImport(AppKit)
            if NSImage(named: name) != nil { return Image(name) }
            #endif
            return nil
        }
        #if canImport(UIKit)
        if UIImage(named: a.still) != nil { return Image(a.still) }
        #elseif canImport(AppKit)
        if NSImage(named: a.still) != nil { return Image(a.still) }
        #endif
        return nil
    }

    static func stillImage(for asset: Asset) -> Image? { still(named: asset.still) }

    // MARK: Hero / loop files

    static func fileURL(_ relative: String) -> URL? {
        let parts = relative.split(separator: "/").map(String.init)
        guard parts.count >= 2 else {
            return Bundle.main.url(forResource: relative, withExtension: nil)
        }
        let folder = parts[0]
        let file = parts[1]
        let name = (file as NSString).deletingPathExtension
        let ext = (file as NSString).pathExtension
        if let u = Bundle.main.url(forResource: name, withExtension: ext, subdirectory: folder) { return u }
        if let root = Bundle.main.resourceURL?.appendingPathComponent(folder).appendingPathComponent(file),
           FileManager.default.fileExists(atPath: root.path) { return root }
        return Bundle.main.url(forResource: name, withExtension: ext)
    }

    // MARK: Picking

    /// Deterministic default: tag match first, else stable hash of `seed`.
    static func pick(kind: Asset.Kind, seed: String, tags: [String] = []) -> Asset? {
        let pool = kind == .agent ? agents : groups
        guard !pool.isEmpty else { return nil }
        let needle = tags.map { $0.lowercased() }.filter { !$0.isEmpty }
        if !needle.isEmpty {
            let scored = pool.map { a -> (Asset, Int) in
                let hits = a.tags.filter { t in needle.contains(where: { $0 == t || $0.contains(t) || t.contains($0) }) }.count
                return (a, hits)
            }.filter { $0.1 > 0 }.sorted { $0.1 > $1.1 }
            if let best = scored.first { return best.0 }
        }
        return pool[stableHash(seed) % pool.count]
    }

    static func pickAgent(handle: String, id: String, hintTags: [String] = []) -> Asset? {
        if let saved = UserDefaults.standard.string(forKey: "RodaAgentAvatar.\(id)"),
           let a = asset(named: saved) { return a }
        var tags = hintTags
        switch handle.lowercased() {
        case "zoen": tags += ["mascot", "assistant", "general"]
        case "financeiro", "finance": tags += ["finance", "budget", "money"]
        case "organizador", "organizer": tags += ["tasks", "productivity", "todo", "calendar"]
        case "guia", "guide": tags += ["travel", "trips", "planner"]
        default: break
        }
        return pick(kind: .agent, seed: id.isEmpty ? handle : id, tags: tags)
    }

    static func pickGroup(spaceId: String, title: String, hintTags: [String] = []) -> Asset? {
        if let saved = UserDefaults.standard.string(forKey: "RodaGroupAvatar.\(spaceId)"),
           let a = asset(named: saved) { return a }
        var tags = hintTags
        let t = title.lowercased()
        let rules: [([String], [String])] = [
            (["viajantes", "litoral", "coastal", "praia", "beach"], ["beach", "trip", "vacation", "summer"]),
            (["hike", "trail", "trilha", "turma", "sábado", "saturday", "crew", "pedra"], ["hiking", "mountains", "outdoors"]),
            (["paraty", "feriado", "viagem"], ["road trip", "travel"]),
            (["produto", "product", "work", "trabalho", "office", "zoen ·"], ["work", "team", "office", "laptop"]),
            (["jantar", "dinner", "cook", "food"], ["cooking", "food", "dinner"]),
            (["music", "música", "band"], ["music", "band"]),
            (["pet", "dog", "cat", "paçoca"], ["pets"]),
            (["work", "trabalho", "office"], ["work", "team", "office"]),
            (["study", "estudo", "school"], ["study", "school"]),
            (["party", "festa"], ["party", "celebration"]),
            (["game", "jogo"], ["games", "gaming"]),
            (["bike", "pedal"], ["cycling", "bike"]),
            (["gym", "treino"], ["gym", "fitness"]),
            (["book", "livro"], ["books", "reading"]),
            (["camp", "acamp"], ["camping", "outdoors"]),
            (["picnic"], ["picnic", "weekend"]),
            (["road"], ["road trip", "travel"]),
            (["city", "cidade"], ["city", "neighborhood"]),
            (["plant", "jardim"], ["plants", "garden"]),
            (["coffee", "café"], ["coffee", "friends"]),
            (["family", "família"], ["family", "dinner"]),
            (["soccer", "futebol"], ["soccer", "sports"]),
            (["birth", "anivers"], ["birthday", "celebration"]),
        ]
        for (keys, tag) in rules where keys.contains(where: t.contains) { tags += tag; break }
        return pick(kind: .group, seed: spaceId.isEmpty ? title : spaceId, tags: tags)
    }

    static func saveGroup(_ spaceId: String, assetName: String) {
        UserDefaults.standard.set(assetName, forKey: "RodaGroupAvatar.\(spaceId)")
    }
    static func saveAgent(_ agentId: String, assetName: String) {
        UserDefaults.standard.set(assetName, forKey: "RodaAgentAvatar.\(agentId)")
    }

    static func stableHash(_ s: String) -> Int {
        Int(s.unicodeScalars.reduce(UInt32(2166136261)) { ($0 ^ $1.value) &* 16777619 } % 9973)
    }

    static func darkModeStyle(_ colorScheme: ColorScheme) -> some ViewModifier {
        DarkMultiply(on: colorScheme == .dark)
    }
}

private struct DarkMultiply: ViewModifier {
    let on: Bool
    func body(content: Content) -> some View {
        if on { content.colorMultiply(Color(red: 0.86, green: 0.84, blue: 0.80)) }
        else { content }
    }
}

/// Decoded loop frames (3 frames, 10 fps WebP), cached per asset.
@MainActor
enum HandDrawnLoopCache {
    private static var frames: [String: [CGImage]] = [:]
    static func frames(for asset: HandDrawnAvatarAsset.Asset) -> [CGImage] {
        if let f = frames[asset.id] { return f }
        guard let url = HandDrawnAvatarAsset.fileURL(asset.loop),
              let src = CGImageSourceCreateWithURL(url as CFURL, nil) else { frames[asset.id] = []; return [] }
        let out = (0..<CGImageSourceGetCount(src)).compactMap { CGImageSourceCreateImageAtIndex(src, $0, nil) }
        frames[asset.id] = out
        return out
    }
}

/// Still in lists; animated WebP loop when large (>= 56 pt) and Reduce Motion is off.
struct HandDrawnAvatarView: View {
    @Environment(\.ambientPaused) private var ambientPaused
    let asset: HandDrawnAvatarAsset.Asset
    var size: CGFloat
    var animate: Bool = true
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.colorScheme) private var colorScheme

    private var wantsLoop: Bool { animate && !reduceMotion && size >= 56 }

    var body: some View {
        content
            .frame(width: size, height: size)
            .clipShape(.circle)
            .modifier(HandDrawnAvatarAsset.darkModeStyle(colorScheme))
            .accessibilityLabel(asset.name)
    }

    @ViewBuilder private var content: some View {
        let frames = wantsLoop ? HandDrawnLoopCache.frames(for: asset) : []
        if frames.count > 1 {
            TimelineView(.animation(minimumInterval: 0.1, paused: ambientPaused)) { ctx in
                let i = Int(ctx.date.timeIntervalSinceReferenceDate * 10) % frames.count
                Image(decorative: frames[i], scale: 1).resizable().interpolation(.high).scaledToFill()
            }
        } else if let img = HandDrawnAvatarAsset.stillImage(for: asset) {
            img.resizable().interpolation(.high).scaledToFill()
        } else {
            Color(hex: asset.backdrop)
        }
    }
}
