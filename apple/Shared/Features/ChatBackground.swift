import SwiftUI
import PhotosUI
import ImageIO
import UniformTypeIdentifiers
import RodaCore
#if canImport(UIKit)
import UIKit
#endif
#if canImport(AppKit)
import AppKit
#endif

// MARK: - Per-chat backgrounds
//
// Chat info → Background: colours, gradients, doodles, 6 built-in paper scenes, or a photo.
// Shared by default via a typed `BackgroundSet` log event (legacy `⟦bg:⟧` markers still parse).
// Photos are resized ≤2048px HEIC/JPEG ~80% and stored content-addressed in the core
// (`put_media`); in shared chats the core sends an encrypted copy through the relay (ADR 0007).

enum ChatBackground: Equatable, Hashable {
    case none
    case color(String)
    case gradient(String)
    case doodles(String)
    case builtin(String)
    case photo(String?)  // sha256 of shared media, or nil for a local-only photo

    var token: String {
        switch self {
        case .none: "none"
        case .color(let k): "color:\(k)"
        case .gradient(let k): "gradient:\(k)"
        case .doodles(let k): "doodles:\(k)"
        case .builtin(let k): "builtin:\(k)"
        case .photo(let sha): sha.map { "photo:\($0)" } ?? "photo"
        }
    }

    init?(token: String) {
        let p = token.split(separator: ":", maxSplits: 1).map(String.init)
        switch (p.first ?? "", p.count > 1 ? p[1] : "") {
        case ("none", _): self = .none
        case ("photo", let sha): self = .photo(sha.isEmpty ? nil : sha)
        case ("color", let k) where Self.colors[k] != nil: self = .color(k)
        case ("gradient", let k) where Self.gradients[k] != nil: self = .gradient(k)
        case ("doodles", let k) where Self.doodleSets[k] != nil: self = .doodles(k)
        case ("builtin", let k) where Self.builtinOrder.contains(k): self = .builtin(k)
        default: return nil
        }
    }

    static let colorOrder = ["butter", "mint", "sky", "lilac", "blush", "cardboard"]
    static let colors: [String: (String, String)] = [
        "butter": ("#FBEFC8", "#2E2715"), "mint": ("#D6F2E5", "#13291F"), "sky": ("#DCEBFF", "#132235"),
        "lilac": ("#E9E2FF", "#221C38"), "blush": ("#FFE3DC", "#33201C"), "cardboard": ("#F1E2CC", "#2B2218"),
    ]
    static let gradientOrder = ["dawn", "forest", "ocean", "dusk", "paper"]
    static let gradients: [String: ([String], [String])] = [
        "dawn": (["#FFE1C7", "#F6D3F0"], ["#3A2420", "#2C1D35"]),
        "forest": (["#E3F5D8", "#BFE3C9"], ["#14261A", "#0E1F1C"]),
        "ocean": (["#D9F1FF", "#C3D8FF"], ["#0F2633", "#151C3A"]),
        "dusk": (["#E8E0FF", "#FFD9E4"], ["#1E1838", "#331B2A"]),
        "paper": (["#FFFCF5", "#F2E6D0"], ["#1C1A17", "#26211A"]),
    ]
    static let doodleOrder = ["trail", "picnic", "studio"]
    static let doodleSets: [String: (glyphs: [ZoenGlyph], tint: String)] = [
        "trail": ([.spaces, .sun, .pin, .heart, .camera], "mint"),
        "picnic": ([.store, .heart, .note, .sparkle, .crown], "butter"),
        "studio": ([.mic, .note, .video, .sparkle, .game], "lilac"),
    ]
    /// Our own hand-drawn paper scenes (bundled asset catalog imagesets).
    static let builtinOrder = ["meadow", "coast", "trail", "dusk", "studio", "paper"]
    static let builtinAsset: [String: String] = [
        "meadow": "bg-meadow", "coast": "bg-coast", "trail": "bg-trail",
        "dusk": "bg-dusk", "studio": "bg-studio", "paper": "bg-paper",
    ]

    var isNone: Bool { self == .none }
    var isPhotoLike: Bool {
        if case .photo = self { return true }
        if case .builtin = self { return true }
        return false
    }

    var label: String {
        switch self {
        case .none: String(localized: "Default")
        case .color(let k), .gradient(let k), .doodles(let k), .builtin(let k): k.capitalized
        case .photo: String(localized: "Photo")
        }
    }

    /// Build from a typed core DTO (shared background).
    init(dto: BackgroundDto) {
        if let bg = ChatBackground(token: dto.style) {
            if case .photo = bg, let sha = dto.media?.sha256 { self = .photo(sha) }
            else { self = bg }
        } else if dto.style == "photo" {
            self = .photo(dto.media?.sha256)
        } else {
            self = .none
        }
    }
}

/// Layout of a photo/builtin over the chat (zoom, offset, dim, blur, appearance).
struct PhotoBackgroundLayout: Equatable, Hashable {
    /// 1000 = 1.0×, 2000 = 2.0×.
    var zoomPm: UInt32 = 1000
    var offsetXPm: Int32 = 0
    var offsetYPm: Int32 = 0
    /// `nil` = auto from luminance under the bubble band.
    var dimPm: UInt32? = nil
    var blurPm: UInt32 = 0
    /// auto | light | dark
    var appearance: String = "auto"

    static let `default` = PhotoBackgroundLayout()

    init() {}
    init(dto: BackgroundDto) {
        zoomPm = dto.zoomPm; offsetXPm = dto.offsetXPm; offsetYPm = dto.offsetYPm
        dimPm = dto.dimPm; blurPm = dto.blurPm; appearance = dto.appearance
    }

    func dto(style: String, media: MediaRefDto?) -> BackgroundDto {
        BackgroundDto(style: style, media: media, zoomPm: zoomPm, offsetXPm: offsetXPm,
                      offsetYPm: offsetYPm, dimPm: dimPm, blurPm: blurPm, appearance: appearance)
    }
}

/// Legacy marker (old builds). New writes use `BackgroundSet`.
enum ChatBackgroundMarker {
    static let open = "⟦bg:", close = "⟧"
    static func text(_ bg: ChatBackground) -> String { "\(open)\(bg.token)\(close)" }
    static func parse(_ text: String) -> ChatBackground? {
        guard text.hasPrefix(open), text.hasSuffix(close) else { return nil }
        return ChatBackground(token: String(text.dropFirst(open.count).dropLast(close.count)))
    }
    static func preview(_ text: String) -> String? {
        text.hasPrefix(open) ? String(localized: "Changed the chat background") : nil
    }
}

// MARK: Store + image cache

@MainActor @Observable
final class ChatBackgroundStore {
    static let shared = ChatBackgroundStore()
    private(set) var overrides: [String: String]
    private(set) var layouts: [String: PhotoBackgroundLayout]
    private(set) var photoVersion = 0
    private let imageCache = NSCache<NSString, PlatformImage>()

    private init() {
        overrides = (UserDefaults.standard.dictionary(forKey: "RodaChatBackgrounds") as? [String: String]) ?? [:]
        if let raw = UserDefaults.standard.data(forKey: "RodaChatBackgroundLayouts"),
           let decoded = try? JSONDecoder().decode([String: LayoutDTO].self, from: raw) {
            layouts = decoded.mapValues { $0.asLayout }
        } else {
            layouts = [:]
        }
        imageCache.countLimit = 24
    }

    func local(_ spaceId: String) -> ChatBackground? {
        if let forced = UserDefaults.standard.string(forKey: "RodaChatBackground") { return ChatBackground(token: forced) }
        return overrides[spaceId].flatMap(ChatBackground.init(token:))
    }

    func setLocal(_ bg: ChatBackground?, layout: PhotoBackgroundLayout? = nil, for spaceId: String) {
        overrides[spaceId] = bg?.token
        UserDefaults.standard.set(overrides, forKey: "RodaChatBackgrounds")
        if let layout { setLayout(layout, for: spaceId) }
    }

    func layout(for spaceId: String) -> PhotoBackgroundLayout { layouts[spaceId] ?? .default }

    func setLayout(_ layout: PhotoBackgroundLayout, for spaceId: String) {
        layouts[spaceId] = layout
        let enc = layouts.mapValues { LayoutDTO($0) }
        if let data = try? JSONEncoder().encode(enc) {
            UserDefaults.standard.set(data, forKey: "RodaChatBackgroundLayouts")
        }
    }

    static func photoURL(_ spaceId: String) -> URL {
        let d = URL.applicationSupportDirectory.appending(path: "Backgrounds", directoryHint: .isDirectory)
        try? FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
        return d.appending(path: "\(spaceId.replacingOccurrences(of: "/", with: "_")).jpg")
    }

    static func mediaURL(_ sha: String) -> URL {
        let d = URL.applicationSupportDirectory.appending(path: "Media", directoryHint: .isDirectory)
        try? FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
        return d.appending(path: "\(sha).bin")
    }

    /// Resizes a picked photo (≤2048px, HEIC or JPEG ~80%), keeps it on this device, and
    /// returns the encoded bytes for the core.
    func savePhoto(_ data: Data, for spaceId: String) -> BackgroundImage.Encoded? {
        guard let encoded = BackgroundImage.encode(data, maxSide: 2048) else { return nil }
        do { try encoded.data.write(to: Self.photoURL(spaceId), options: .completeFileProtection) } catch { return nil }
        photoVersion += 1
        imageCache.removeAllObjects()
        return encoded
    }

    func hasMedia(_ sha: String) -> Bool { FileManager.default.fileExists(atPath: Self.mediaURL(sha).path) }

    func cacheMedia(_ sha: String, bytes: Data) {
        try? bytes.write(to: Self.mediaURL(sha), options: .completeFileProtection)
        photoVersion += 1
        imageCache.removeAllObjects()
    }

    func platformImage(for background: ChatBackground, spaceId: String, maxPixel: CGFloat) -> PlatformImage? {
        _ = photoVersion
        let key: String
        let url: URL?
        switch background {
        case .photo(let sha):
            if let sha, FileManager.default.fileExists(atPath: Self.mediaURL(sha).path) {
                key = "m:\(sha):\(Int(maxPixel))"; url = Self.mediaURL(sha)
            } else {
                key = "p:\(spaceId):\(Int(maxPixel))"; url = Self.photoURL(spaceId)
            }
        case .builtin(let k):
            key = "b:\(k):\(Int(maxPixel))"
            if let cached = imageCache.object(forKey: key as NSString) { return cached }
            guard let name = ChatBackground.builtinAsset[k],
                  let img = PlatformImage.named(name) else { return nil }
            let down = BackgroundImage.downsample(img, maxPixel: maxPixel) ?? img
            imageCache.setObject(down, forKey: key as NSString)
            return down
        default:
            return nil
        }
        if let cached = imageCache.object(forKey: key as NSString) { return cached }
        guard let url, let img = BackgroundImage.thumbnail(url: url, maxPixel: maxPixel) else { return nil }
        imageCache.setObject(img, forKey: key as NSString)
        return img
    }

    func swiftImage(for background: ChatBackground, spaceId: String, maxPixel: CGFloat = 1200) -> Image? {
        guard let img = platformImage(for: background, spaceId: spaceId, maxPixel: maxPixel) else { return nil }
        #if os(iOS)
        return Image(uiImage: img)
        #else
        return Image(nsImage: img)
        #endif
    }

    private struct LayoutDTO: Codable {
        var zoomPm: UInt32; var offsetXPm: Int32; var offsetYPm: Int32
        var dimPm: UInt32?; var blurPm: UInt32; var appearance: String
        init(_ l: PhotoBackgroundLayout) {
            zoomPm = l.zoomPm; offsetXPm = l.offsetXPm; offsetYPm = l.offsetYPm
            dimPm = l.dimPm; blurPm = l.blurPm; appearance = l.appearance
        }
        var asLayout: PhotoBackgroundLayout {
            var l = PhotoBackgroundLayout(); l.zoomPm = zoomPm; l.offsetXPm = offsetXPm
            l.offsetYPm = offsetYPm; l.dimPm = dimPm; l.blurPm = blurPm; l.appearance = appearance
            return l
        }
    }
}

// MARK: Image helpers

#if os(iOS)
typealias PlatformImage = UIImage
extension UIImage {
    static func named(_ name: String) -> UIImage? { UIImage(named: name) }
    var cgImageRef: CGImage? { cgImage }
    var pixelSize: CGSize { CGSize(width: size.width * scale, height: size.height * scale) }
}
#else
typealias PlatformImage = NSImage
extension NSImage {
    static func named(_ name: String) -> NSImage? { NSImage(named: name) }
    var cgImageRef: CGImage? { cgImage(forProposedRect: nil, context: nil, hints: nil) }
    var pixelSize: CGSize { CGSize(width: size.width, height: size.height) }
}
#endif

enum BackgroundImage {
    struct Encoded { let data: Data; let mime: String; let size: CGSize }

    /// HEIC preferred, JPEG fallback, max side 2048, ~0.8 quality.
    static func encode(_ data: Data, maxSide: CGFloat) -> Encoded? {
        guard let src = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
              let props = CGImageSourceCopyPropertiesAtIndex(src, 0, nil) as? [CFString: Any],
              let w = props[kCGImagePropertyPixelWidth] as? CGFloat,
              let h = props[kCGImagePropertyPixelHeight] as? CGFloat else {
            #if os(iOS)
            guard let img = UIImage(data: data) else { return nil }
            let s = min(1, maxSide / max(img.size.width, img.size.height))
            let target = CGSize(width: img.size.width * s, height: img.size.height * s)
            let out = UIGraphicsImageRenderer(size: target).image { _ in img.draw(in: CGRect(origin: .zero, size: target)) }
            guard let jpg = out.jpegData(compressionQuality: 0.8) else { return nil }
            return Encoded(data: jpg, mime: "image/jpeg", size: target)
            #else
            return nil
            #endif
        }
        let scale = min(1, maxSide / max(w, h))
        let tw = max(1, Int(w * scale)), th = max(1, Int(h * scale))
        let opts: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: max(tw, th),
            kCGImageSourceShouldCacheImmediately: true,
        ]
        guard let cg = CGImageSourceCreateThumbnailAtIndex(src, 0, opts as CFDictionary) else { return nil }
        let dest = NSMutableData()
        let heic = UTType.heic.identifier as CFString
        if let destRef = CGImageDestinationCreateWithData(dest, heic, 1, nil) {
            CGImageDestinationAddImage(destRef, cg, [kCGImageDestinationLossyCompressionQuality: 0.8] as CFDictionary)
            if CGImageDestinationFinalize(destRef), dest.length > 0 {
                return Encoded(data: dest as Data, mime: "image/heic", size: CGSize(width: tw, height: th))
            }
        }
        dest.length = 0
        let jpeg = UTType.jpeg.identifier as CFString
        guard let destRef = CGImageDestinationCreateWithData(dest, jpeg, 1, nil) else { return nil }
        CGImageDestinationAddImage(destRef, cg, [kCGImageDestinationLossyCompressionQuality: 0.8] as CFDictionary)
        guard CGImageDestinationFinalize(destRef) else { return nil }
        return Encoded(data: dest as Data, mime: "image/jpeg", size: CGSize(width: tw, height: th))
    }

    static func thumbnail(url: URL, maxPixel: CGFloat) -> PlatformImage? {
        guard let src = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary) else {
            return PlatformImage(contentsOfFile: url.path).flatMap { downsample($0, maxPixel: maxPixel) }
        }
        let opts: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: Int(maxPixel),
            kCGImageSourceShouldCacheImmediately: true,
        ]
        guard let cg = CGImageSourceCreateThumbnailAtIndex(src, 0, opts as CFDictionary) else { return nil }
        #if os(iOS)
        return UIImage(cgImage: cg)
        #else
        return NSImage(cgImage: cg, size: NSSize(width: cg.width, height: cg.height))
        #endif
    }

    static func downsample(_ img: PlatformImage, maxPixel: CGFloat) -> PlatformImage? {
        guard let cg = img.cgImageRef else { return img }
        let maxSide = CGFloat(max(cg.width, cg.height))
        guard maxSide > maxPixel else { return img }
        let s = maxPixel / maxSide
        let tw = max(1, Int(CGFloat(cg.width) * s)), th = max(1, Int(CGFloat(cg.height) * s))
        #if os(iOS)
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        return UIGraphicsImageRenderer(size: CGSize(width: tw, height: th), format: format).image { _ in
            UIImage(cgImage: cg).draw(in: CGRect(x: 0, y: 0, width: tw, height: th))
        }
        #else
        let out = NSImage(size: NSSize(width: tw, height: th))
        out.lockFocus(); defer { out.unlockFocus() }
        img.draw(in: NSRect(x: 0, y: 0, width: tw, height: th), from: .zero, operation: .copy, fraction: 1)
        return out
        #endif
    }

    /// Mean luminance 0…1 of the middle band (where bubbles sit).
    static func bubbleBandLuminance(_ img: PlatformImage) -> Double {
        guard let cg = img.cgImageRef else { return 0.5 }
        let tw = 32, th = 32
        var buf = [UInt8](repeating: 0, count: tw * th * 4)
        guard let ctx = CGContext(data: &buf, width: tw, height: th, bitsPerComponent: 8, bytesPerRow: tw * 4,
                                  space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return 0.5 }
        // Sample the vertical middle third (composer + messages band).
        let srcH = CGFloat(cg.height), srcW = CGFloat(cg.width)
        let band = CGRect(x: 0, y: srcH * 0.35, width: srcW, height: srcH * 0.4)
        ctx.draw(cg, in: CGRect(x: -band.minX / band.width * CGFloat(tw),
                                y: -band.minY / band.height * CGFloat(th),
                                width: srcW / band.width * CGFloat(tw),
                                height: srcH / band.height * CGFloat(th)))
        var sum = 0.0
        for i in stride(from: 0, to: buf.count, by: 4) {
            let r = Double(buf[i]) / 255, g = Double(buf[i + 1]) / 255, b = Double(buf[i + 2]) / 255
            sum += 0.2126 * r + 0.7152 * g + 0.0722 * b
        }
        return sum / Double(tw * th)
    }

    /// Auto dim 0…1 from the luminance under the bubbles. Light mode: bubbles are opaque
    /// white/green, so only a light veil on bright photos. Dark mode: bright photos glare
    /// against dark bubbles, so dim harder.
    static func autoDim(luminance: Double, dark: Bool) -> Double {
        dark ? min(0.65, max(0.22, 0.2 + luminance * 0.45))
             : min(0.2, max(0.04, 0.06 + (luminance - 0.55) * 0.3))
    }

    /// Bubble shadow strength for the backdrop: busier/brighter → stronger.
    static func bubbleShadow(luminance: Double, dark: Bool) -> Double {
        dark ? 0.25 : (luminance > 0.7 ? 0.16 : 0.1)
    }
}

// MARK: Environment

extension EnvironmentValues {
    @Entry var chatBackdrop: Bool = false
}

// MARK: Backdrop view

struct ChatBackdropView: View {
    let background: ChatBackground
    let spaceId: String
    var layout: PhotoBackgroundLayout = .default
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        let dark = scheme == .dark
        switch background {
        case .none:
            Palette.background
        case .color(let k):
            let c = ChatBackground.colors[k] ?? ("#FFFFFF", "#000000")
            Color(hex: dark ? c.1 : c.0)
        case .gradient(let k):
            let g = ChatBackground.gradients[k] ?? (["#FFFFFF"], ["#000000"])
            LinearGradient(colors: (dark ? g.1 : g.0).map { Color(hex: $0) }, startPoint: .topLeading, endPoint: .bottomTrailing)
        case .doodles(let k):
            DoodleBackdrop(set: k)
        case .photo, .builtin:
            PhotoBackdrop(background: background, spaceId: spaceId, layout: layout)
        }
    }
}

struct PhotoBackdrop: View {
    let background: ChatBackground
    let spaceId: String
    var layout: PhotoBackgroundLayout
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        GeometryReader { g in
            let img = ChatBackgroundStore.shared.swiftImage(for: background, spaceId: spaceId,
                                                            maxPixel: max(g.size.width, g.size.height) * 2)
            let zoom = CGFloat(layout.zoomPm) / 1000
            let ox = CGFloat(layout.offsetXPm) / 1000 * g.size.width
            let oy = CGFloat(layout.offsetYPm) / 1000 * g.size.height
            let lum: Double = {
                if let p = ChatBackgroundStore.shared.platformImage(for: background, spaceId: spaceId, maxPixel: 64) {
                    return BackgroundImage.bubbleBandLuminance(p)
                }
                return 0.5
            }()
            let dim = Double(layout.dimPm.map { Double($0) / 1000 } ?? BackgroundImage.autoDim(luminance: lum, dark: scheme == .dark))
            let blur = CGFloat(layout.blurPm) / 1000 * 20
            ZStack {
                if let img {
                    img.resizable().scaledToFill()
                        .scaleEffect(zoom)
                        .offset(x: ox, y: oy)
                        .frame(width: g.size.width, height: g.size.height)
                        .clipped()
                        .blur(radius: blur)
                } else {
                    Palette.background
                }
                Color.black.opacity(dim)
            }
        }
        .ignoresSafeArea()
    }
}

struct DoodleBackdrop: View {
    let set: String
    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let spec = ChatBackground.doodleSets[set] ?? ChatBackground.doodleSets["trail"]!
        let dark = scheme == .dark
        let base = ChatBackground.colors[spec.tint] ?? ("#FFFFFF", "#000000")
        let ink = dark ? Color.white.opacity(0.13) : InkPalette.ink.opacity(0.12)
        TimelineView(.animation(minimumInterval: 1 / 30, paused: reduceMotion)) { tl in
            let t = reduceMotion ? 0 : tl.date.timeIntervalSinceReferenceDate
            Canvas { ctx, size in
                let cell: CGFloat = 74
                let drift = CGFloat(t.truncatingRemainder(dividingBy: 600)) * 4
                let cols = Int(size.width / cell) + 3, rows = Int(size.height / cell) + 3
                for r in -1..<rows {
                    for c in -1..<cols {
                        let k = abs((r * 7 + c * 3)) % spec.glyphs.count
                        guard let sym = ctx.resolveSymbol(id: k) else { continue }
                        let stagger: CGFloat = r.isMultiple(of: 2) ? 0 : cell / 2
                        var x = CGFloat(c) * cell + stagger + drift.truncatingRemainder(dividingBy: cell)
                        let y = CGFloat(r) * cell + (drift * 0.5).truncatingRemainder(dividingBy: cell)
                        x -= cell
                        let phase = Double(r * 13 + c * 5)
                        let bob = reduceMotion ? 0 : sin(t * 0.9 + phase) * 3
                        let rot = Angle.degrees(Double((r * 31 + c * 17) % 40) - 20 + (reduceMotion ? 0 : sin(t * 0.6 + phase) * 6))
                        var g = ctx
                        g.translateBy(x: x, y: y + bob)
                        g.rotate(by: rot)
                        g.draw(sym, at: .zero)
                    }
                }
            } symbols: {
                ForEach(Array(spec.glyphs.enumerated()), id: \.offset) { i, glyph in
                    InkIcon(glyph: glyph, direction: .doodle, size: 30).foregroundStyle(ink).tag(i)
                }
            }
        }
        .background(Color(hex: dark ? base.1 : base.0))
        .accessibilityHidden(true)
    }
}

// MARK: Picker

struct ChatBackgroundPicker: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let spaceId: String
    let current: ChatBackground
    @State private var choice: ChatBackground
    @State private var justForMe: Bool
    @State private var photoItem: PhotosPickerItem?
    @State private var hasPhoto = false
    @State private var pendingPhoto: (Data, CGSize, String)?  // bytes, size, mime
    @State private var editor: PhotoEditorLaunch?

    init(spaceId: String, current: ChatBackground, isLocal: Bool) {
        self.spaceId = spaceId
        self.current = current
        _choice = State(initialValue: current)
        _justForMe = State(initialValue: isLocal)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    preview
                    // Prominent Photo entry (iMessage-style).
                    photoRow
                    section("Scenes") {
                        ForEach(ChatBackground.builtinOrder, id: \.self) { swatch(.builtin($0)) }
                    }
                    section("Colors") {
                        swatch(.none)
                        ForEach(ChatBackground.colorOrder, id: \.self) { swatch(.color($0)) }
                    }
                    section("Gradients") { ForEach(ChatBackground.gradientOrder, id: \.self) { swatch(.gradient($0)) } }
                    section("Animated doodles") { ForEach(ChatBackground.doodleOrder, id: \.self) { swatch(.doodles($0)) } }
                    VStack(alignment: .leading, spacing: 6) {
                        Toggle(isOn: $justForMe) { Text("Just for me").font(.body.weight(.medium)) }
                        Text(justForMe
                             ? String(localized: "Only you see this background.")
                             : String(localized: "Everyone in the chat sees this background. It’s saved in the chat’s signed log."))
                            .font(.footnote)
                            .foregroundStyle(Palette.textSecondary)
                    }
                    .tint(Palette.action)
                }
                .padding(20)
            }
            .background(Palette.background)
            .navigationTitle("Background")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Set") { openEditorIfNeeded() }.fontWeight(.semibold)
                }
            }
        }
        .onAppear {
            hasPhoto = FileManager.default.fileExists(atPath: ChatBackgroundStore.photoURL(spaceId).path)
                || {
                    if case .photo(let sha) = current, let sha {
                        return FileManager.default.fileExists(atPath: ChatBackgroundStore.mediaURL(sha).path)
                    }
                    return false
                }()
        }
        .onChange(of: photoItem) { _, item in
            guard let item else { return }
            Task {
                if let data = try? await item.loadTransferable(type: Data.self),
                   let enc = ChatBackgroundStore.shared.savePhoto(data, for: spaceId) {
                    hasPhoto = true
                    pendingPhoto = (enc.data, enc.size, enc.mime)
                    choice = .photo(nil)
                    editor = PhotoEditorLaunch(background: .photo(nil), layout: ChatBackgroundStore.shared.layout(for: spaceId))
                }
            }
        }
        #if os(iOS)
        .fullScreenCover(item: $editor) { launch in editorView(launch) }
        #else
        .sheet(item: $editor) { launch in editorView(launch).frame(minWidth: 420, minHeight: 760) }
        #endif
        .task {
            // Screenshots: `-RodaBackgroundEditor builtin:coast` opens the position editor.
            guard let t = UserDefaults.standard.string(forKey: "RodaBackgroundEditor"), let bg = ChatBackground(token: t) else { return }
            try? await Task.sleep(for: .seconds(1.2))
            choice = bg
            editor = PhotoEditorLaunch(background: bg, layout: ChatBackgroundStore.shared.layout(for: spaceId))
        }
    }

    private func editorView(_ launch: PhotoEditorLaunch) -> some View {
        PhotoBackgroundEditor(spaceId: spaceId, background: launch.background,
                              initial: launch.layout, pendingPhoto: pendingPhoto,
                              title: model.space(spaceId)?.title ?? "",
                              messages: recentMessages()) { layout, media in
            editor = nil
            apply(layout: layout, media: media)
        } onCancel: { editor = nil }
    }

    /// The chat's last few messages, for the live preview in the editor.
    private func recentMessages() -> [EditorMessage] {
        let entries = (try? model.core.timeline(spaceId: spaceId)) ?? []
        var out: [EditorMessage] = []
        for e in entries.reversed() {
            guard case .message(let text, _) = e.kind, ChatBackgroundMarker.parse(text) == nil else { continue }
            let shown = VoiceNoteRef.preview(text) ?? text
            out.append(EditorMessage(id: e.id, text: String(shown.prefix(140)), mine: e.author.isMe, name: e.author.name))
            if out.count == 5 { break }
        }
        if out.isEmpty {
            out = [EditorMessage(id: "a", text: String(localized: "Saturday works for me"), mine: false, name: ""),
                   EditorMessage(id: "b", text: String(localized: "Same! I'll bring snacks"), mine: true, name: "")]
        }
        return out.reversed()
    }

    private var photoRow: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Photo").font(.subheadline.weight(.semibold)).foregroundStyle(Palette.textSecondary)
            HStack(spacing: 12) {
                PhotosPicker(selection: $photoItem, matching: .images) {
                    Label {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Choose a Photo").font(.body.weight(.semibold))
                            Text("No library access needed").font(.caption).foregroundStyle(Palette.textSecondary)
                        }
                    } icon: {
                        Image(systemName: "photo.on.rectangle")
                            .font(.title2)
                            .foregroundStyle(Palette.action)
                            .frame(width: 44, height: 44)
                            .background(Palette.action.opacity(0.12), in: .circle)
                    }
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(.regularMaterial, in: .rect(cornerRadius: 16, style: .continuous))
                }
                .buttonStyle(.plain)
                if hasPhoto {
                    Button {
                        choice = .photo(nil)
                        editor = PhotoEditorLaunch(background: .photo(nil), layout: ChatBackgroundStore.shared.layout(for: spaceId))
                    } label: {
                        ChatBackdropView(background: .photo(nil), spaceId: spaceId)
                            .frame(width: 64, height: 88)
                            .clipShape(.rect(cornerRadius: 14, style: .continuous))
                            .overlay {
                                RoundedRectangle(cornerRadius: 14, style: .continuous)
                                    .stroke(choice.isPhotoLike && { if case .photo = choice { return true }; return false }()
                                            ? Palette.action : Palette.textPrimary.opacity(0.1),
                                            lineWidth: { if case .photo = choice { return 3 }; return 1 }())
                            }
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(Text("Photo"))
                }
            }
        }
    }

    private var preview: some View {
        ZStack {
            ChatBackdropView(background: choice, spaceId: spaceId, layout: ChatBackgroundStore.shared.layout(for: spaceId))
            VStack(alignment: .leading, spacing: 8) {
                PreviewBubble(text: String(localized: "Saturday works for me"), mine: false)
                PreviewBubble(text: String(localized: "Same! I'll bring snacks"), mine: true)
                PreviewBubble(text: String(localized: "Meet at 8 at the trailhead"), mine: false)
            }
            .environment(\.chatBackdrop, !choice.isNone)
            .padding(14)
        }
        .frame(height: 190)
        .clipShape(.rect(cornerRadius: 22, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 22, style: .continuous).stroke(Palette.textPrimary.opacity(0.08)))
        .animation(.smooth, value: choice)
    }

    private func section<C: View>(_ title: LocalizedStringKey, @ViewBuilder _ content: () -> C) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.textSecondary)
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 10) { content() }.padding(.vertical, 3)
            }
            .scrollClipDisabled()
        }
    }

    private func swatch(_ bg: ChatBackground) -> some View {
        Button {
            Haptics.selectionTick(); choice = bg
            if bg.isPhotoLike {
                editor = PhotoEditorLaunch(background: bg, layout: ChatBackgroundStore.shared.layout(for: spaceId))
            }
        } label: {
            ChatBackdropView(background: bg, spaceId: spaceId)
                .frame(width: 64, height: 88)
                .clipShape(.rect(cornerRadius: 14, style: .continuous))
                .overlay {
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .stroke(choice == bg ? Palette.action : Palette.textPrimary.opacity(0.1), lineWidth: choice == bg ? 3 : 1)
                }
                .overlay(alignment: .bottom) {
                    if bg.isNone { Text("Default").font(.caption2.weight(.semibold)).foregroundStyle(Palette.textSecondary).padding(.bottom, 6) }
                }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(bg.label))
        .accessibilityAddTraits(choice == bg ? .isSelected : [])
    }

    private func openEditorIfNeeded() {
        if choice.isPhotoLike {
            editor = PhotoEditorLaunch(background: choice, layout: ChatBackgroundStore.shared.layout(for: spaceId))
        } else {
            apply(layout: .default, media: nil)
        }
    }

    private func apply(layout: PhotoBackgroundLayout, media: MediaRefDto?) {
        let store = ChatBackgroundStore.shared
        store.setLayout(layout, for: spaceId)
        if justForMe {
            store.setLocal(choice, layout: layout, for: spaceId)
            Haptics.commit(); dismiss(); return
        }
        store.setLocal(nil, for: spaceId)
        let style: String = {
            switch choice {
            case .none: return "none"
            case .color(let k): return "color:\(k)"
            case .gradient(let k): return "gradient:\(k)"
            case .doodles(let k): return "doodles:\(k)"
            case .builtin(let k): return "builtin:\(k)"
            case .photo: return "photo"
            }
        }()
        var mediaRef = media
        if case .photo = choice, mediaRef == nil, let pending = pendingPhoto {
            mediaRef = model.perform {
                try model.core.putMedia(bytes: pending.0, mime: pending.2,
                                        width: UInt32(pending.1.width), height: UInt32(pending.1.height))
            }
            if let m = mediaRef {
                store.cacheMedia(m.sha256, bytes: pending.0)
            }
        }
        if case .photo = choice, mediaRef == nil {
            // Local photo without core: fall back to just-for-me.
            store.setLocal(.photo(nil), layout: layout, for: spaceId)
            Haptics.commit(); dismiss(); return
        }
        let dto = layout.dto(style: style, media: mediaRef)
        model.perform { try model.core.setBackground(spaceId: spaceId, background: dto) }
        if let m = mediaRef { choice = .photo(m.sha256) }
        Haptics.commit()
        dismiss()
    }
}

struct EditorMessage: Identifiable {
    let id: String
    let text: String
    let mine: Bool
    let name: String
}

private struct PhotoEditorLaunch: Identifiable {
    let id = UUID()
    let background: ChatBackground
    let layout: PhotoBackgroundLayout
}

// MARK: Position editor (iMessage-style)

struct PhotoBackgroundEditor: View {
    let spaceId: String
    let background: ChatBackground
    let initial: PhotoBackgroundLayout
    let pendingPhoto: (Data, CGSize, String)?
    var title: String = ""
    var messages: [EditorMessage] = []
    var onSet: (PhotoBackgroundLayout, MediaRefDto?) -> Void
    var onCancel: () -> Void

    @State private var layout: PhotoBackgroundLayout
    @State private var dimManual: Double
    @State private var useAutoDim: Bool
    @State private var blur: Double
    @State private var appearance: String
    @GestureState private var pinch: CGFloat = 1
    @GestureState private var drag: CGSize = .zero
    @State private var baseZoom: CGFloat
    @State private var baseOffset: CGSize
    @Environment(\.colorScheme) private var systemScheme

    init(spaceId: String, background: ChatBackground, initial: PhotoBackgroundLayout,
         pendingPhoto: (Data, CGSize, String)?,
         title: String = "", messages: [EditorMessage] = [],
         onSet: @escaping (PhotoBackgroundLayout, MediaRefDto?) -> Void,
         onCancel: @escaping () -> Void) {
        self.spaceId = spaceId; self.background = background; self.initial = initial
        self.pendingPhoto = pendingPhoto; self.onSet = onSet; self.onCancel = onCancel
        self.title = title; self.messages = messages
        _layout = State(initialValue: initial)
        _useAutoDim = State(initialValue: initial.dimPm == nil)
        _dimManual = State(initialValue: Double(initial.dimPm ?? 300) / 1000)
        _blur = State(initialValue: Double(initial.blurPm) / 1000)
        _appearance = State(initialValue: initial.appearance)
        _baseZoom = State(initialValue: CGFloat(initial.zoomPm) / 1000)
        _baseOffset = State(initialValue: CGSize(width: CGFloat(initial.offsetXPm) / 1000,
                                                 height: CGFloat(initial.offsetYPm) / 1000))
    }

    var body: some View {
        let live = liveLayout()
        ZStack {
            ChatBackdropView(background: background, spaceId: spaceId, layout: live)
                .ignoresSafeArea()
                .gesture(SimultaneousGesture(
                    MagnifyGesture().updating($pinch) { v, s, _ in s = v.magnification }
                        .onEnded { v in baseZoom = min(5, max(1, baseZoom * v.magnification)); sync() },
                    DragGesture().updating($drag) { v, s, _ in s = v.translation }
                        .onEnded { v in
                            baseOffset.width = (baseOffset.width + v.translation.width / UIScreenWidth).clamped(to: -1...1)
                            baseOffset.height = (baseOffset.height + v.translation.height / UIScreenHeight).clamped(to: -1...1)
                            sync()
                        }
                ))
            VStack(spacing: 10) {
                HStack {
                    Button("Cancel") { onCancel() }
                        .padding(.horizontal, 14).padding(.vertical, 8)
                        .glassEffect(.regular.interactive(), in: .capsule)
                    Spacer()
                    // The chat's glass top bar, refracting the photo.
                    Text(title.isEmpty ? String(localized: "Move and Scale") : title)
                        .font(.subheadline.weight(.semibold))
                        .lineLimit(1)
                        .padding(.horizontal, 14).padding(.vertical, 8)
                        .glassEffect(.regular, in: .capsule)
                    Spacer()
                    Button("Set") { commit() }
                        .fontWeight(.semibold)
                        .padding(.horizontal, 14).padding(.vertical, 8)
                        .glassEffect(.regular.tint(Palette.action).interactive(), in: .capsule)
                        .foregroundStyle(.white)
                        .accessibilityIdentifier("bg-editor-set")
                }
                .foregroundStyle(Palette.textPrimary)
                .padding(.horizontal, 16).padding(.top, 8)
                Text("Pinch to zoom · drag to move")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white)
                    .shadow(color: .black.opacity(0.4), radius: 3)
                Spacer()
                // Live bubbles: the chat's real last messages.
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(messages) { m in
                        VStack(alignment: .leading, spacing: 2) {
                            if !m.mine && !m.name.isEmpty {
                                Text(m.name).font(.caption2.weight(.semibold)).foregroundStyle(Palette.textSecondary)
                                    .padding(.horizontal, 7).padding(.vertical, 1)
                                    .background(.regularMaterial, in: .capsule)
                                    .padding(.leading, 6)
                            }
                            PreviewBubble(text: m.text, mine: m.mine)
                        }
                    }
                }
                .environment(\.chatBackdrop, true)
                .padding(.horizontal, 14)
                .allowsHitTesting(false)
                // Composer glass pill, refracting the photo.
                HStack {
                    Text("Message").foregroundStyle(Palette.textSecondary)
                    Spacer()
                    Image(systemName: "mic.fill").foregroundStyle(Palette.textSecondary)
                }
                .padding(.horizontal, 16).padding(.vertical, 11)
                .glassEffect(.regular, in: .capsule)
                .padding(.horizontal, 14)
                .allowsHitTesting(false)
                controls
            }
        }
        .environment(\.colorScheme, appearance == "light" ? .light : appearance == "dark" ? .dark : systemScheme)
    }

    private var controls: some View {
        VStack(spacing: 14) {
            HStack {
                Text("Dim").font(.caption.weight(.semibold)).foregroundStyle(Palette.textSecondary).frame(width: 40, alignment: .leading)
                Toggle(isOn: $useAutoDim) { Text("Auto") }
                    .labelsHidden()
                    .tint(Palette.action)
                    .accessibilityLabel(Text("Automatic dim"))
                    .onChange(of: useAutoDim) { _, _ in sync() }
                if !useAutoDim {
                    Slider(value: $dimManual, in: 0...0.7).tint(Palette.action)
                        .onChange(of: dimManual) { _, _ in sync() }
                } else {
                    Text("Auto · from the photo").font(.caption).foregroundStyle(Palette.textSecondary)
                    Spacer()
                }
            }
            HStack {
                Text("Blur").font(.caption.weight(.semibold)).foregroundStyle(Palette.textSecondary).frame(width: 40, alignment: .leading)
                Slider(value: $blur, in: 0...1).tint(Palette.action)
                    .onChange(of: blur) { _, _ in sync() }
            }
            Picker("Appearance", selection: $appearance) {
                Text("Auto").tag("auto")
                Text("Light").tag("light")
                Text("Dark").tag("dark")
            }
            .pickerStyle(.segmented)
            .onChange(of: appearance) { _, _ in sync() }
        }
        .padding(16)
        .glassEffect(.regular, in: .rect(cornerRadius: 22, style: .continuous))
        .padding(.horizontal, 14)
        .padding(.bottom, 12)
    }

    private func liveLayout() -> PhotoBackgroundLayout {
        var l = layout
        l.zoomPm = UInt32(min(5, max(1, baseZoom * pinch)) * 1000)
        let w = UIScreenWidth, h = UIScreenHeight
        l.offsetXPm = Int32(((baseOffset.width + drag.width / w).clamped(to: -1...1)) * 1000)
        l.offsetYPm = Int32(((baseOffset.height + drag.height / h).clamped(to: -1...1)) * 1000)
        l.dimPm = useAutoDim ? nil : UInt32(dimManual * 1000)
        l.blurPm = UInt32(blur * 1000)
        l.appearance = appearance
        return l
    }

    private func sync() { layout = liveLayout() }

    private func commit() {
        let final = liveLayout()
        onSet(final, nil)
    }
}

#if os(iOS)
private var UIScreenWidth: CGFloat { UIScreen.main.bounds.width }
private var UIScreenHeight: CGFloat { UIScreen.main.bounds.height }
#else
private var UIScreenWidth: CGFloat { 400 }
private var UIScreenHeight: CGFloat { 800 }
#endif

private extension Comparable {
    func clamped(to r: ClosedRange<Self>) -> Self { min(max(self, r.lowerBound), r.upperBound) }
}

private struct PreviewBubble: View {
    let text: String
    let mine: Bool
    @Environment(\.chatBackdrop) private var backdrop
    var body: some View {
        HStack {
            if mine { Spacer() }
            Text(text)
                .font(.subheadline)
                .foregroundStyle(mine ? Palette.myBubbleText : Palette.textPrimary)
                .padding(.horizontal, 12).padding(.vertical, 7)
                .background(mine ? Palette.myBubble : (backdrop ? Palette.otherBubbleOnBackdrop : Palette.otherBubble), in: .rect(cornerRadius: 16, style: .continuous))
                .shadow(color: .black.opacity(backdrop ? 0.08 : 0), radius: 1.5, y: 0.5)
            if !mine { Spacer() }
        }
    }
}

/// "Enzo set a photo as the background" — typed event, with thumbnail.
struct BackgroundChangeRow: View {
    let author: Persona
    let background: ChatBackground
    let spaceId: String
    var layout: PhotoBackgroundLayout = .default
    @State private var preview = false

    var body: some View {
        Button { preview = true } label: {
            HStack(spacing: 8) {
                ChatBackdropView(background: background, spaceId: spaceId, layout: layout)
                    .frame(width: 22, height: 22)
                    .clipShape(.rect(cornerRadius: 5, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 5).stroke(Palette.textPrimary.opacity(0.15)))
                Text(label)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(Palette.textSecondary)
            }
            .padding(.horizontal, 12).padding(.vertical, 6)
            .background(.regularMaterial, in: .capsule)
        }
        .buttonStyle(.plain)
        .frame(maxWidth: .infinity)
        .padding(.vertical, 8)
        .sheet(isPresented: $preview) {
            NavigationStack {
                ChatBackdropView(background: background, spaceId: spaceId, layout: layout)
                    .ignoresSafeArea()
                    .navigationTitle(String(localized: "Background"))
                    #if os(iOS)
                    .navigationBarTitleDisplayMode(.inline)
                    #endif
                    .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { preview = false } } }
            }
        }
        .accessibilityHint(Text("Shows the background"))
    }

    private var label: String {
        let photo = background.isPhotoLike
        if author.isMe {
            return photo ? String(localized: "You set a photo as the background")
                         : String(localized: "You changed the chat background")
        }
        return photo ? String(localized: "\(author.name) set a photo as the background")
                     : String(localized: "\(author.name) changed the chat background")
    }
}

extension View {
    func menuPreviewShape<S: Shape>(_ shape: S) -> some View {
        #if os(iOS)
        contentShape(.contextMenuPreview, shape)
        #else
        contentShape(shape)
        #endif
    }
}
