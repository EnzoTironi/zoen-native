import SwiftUI
import ClientCore

#if canImport(UIKit)
import UIKit
typealias PlatformColor = UIColor
#else
import AppKit
typealias PlatformColor = NSColor
#endif

// MARK: - Cores

extension Color {
    init(hex: String) {
        self.init(platformHex: hex)
    }

    fileprivate init(platformHex hex: String) {
        self = Color(PlatformColor(hex: hex))
    }

    /// Cor que muda entre claro e escuro.
    static func adaptive(light: String, dark: String) -> Color {
        #if canImport(UIKit)
        Color(UIColor { $0.userInterfaceStyle == .dark ? UIColor(hex: dark) : UIColor(hex: light) })
        #else
        Color(NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? NSColor(hex: dark) : NSColor(hex: light)
        })
        #endif
    }
}

extension PlatformColor {
    convenience init(hex: String) {
        var s = hex.trimmingCharacters(in: .whitespacesAndNewlines)
        if s.hasPrefix("#") { s.removeFirst() }
        var v: UInt64 = 0
        Scanner(string: s).scanHexInt64(&v)
        let r = CGFloat((v >> 16) & 0xFF) / 255, g = CGFloat((v >> 8) & 0xFF) / 255, b = CGFloat(v & 0xFF) / 255
        self.init(red: r, green: g, blue: b, alpha: 1)
    }
}

/// Shared colors for Light and Dark appearance.
enum Palette {
    static let background = Color.adaptive(light: "#F4F6FB", dark: "#090D15")
    static let surface = Color.adaptive(light: "#FFFFFF", dark: "#121826")
    static let surfaceRaised = Color.adaptive(light: "#FFFFFF", dark: "#182032")
    static let surfaceMuted = Color.adaptive(light: "#ECEFF6", dark: "#1A2233")
    static let hairline = Color.adaptive(light: "#E1E5EE", dark: "#253047")
    static let textPrimary = Color.adaptive(light: "#0E1320", dark: "#EEF2FA")
    static let textSecondary = Color.adaptive(light: "#5B6478", dark: "#93A0B8")
    static let textTertiary = Color.adaptive(light: "#8C95A8", dark: "#5F6B82")
    static let action = Color.adaptive(light: "#3D7A28", dark: "#86C25A")  // moss green: the mascot's fur
    static let actionDeep = Color.adaptive(light: "#2C5E1C", dark: "#5E9A35")
    static let onAction = Color.adaptive(light: "#FFFFFF", dark: "#0E1320")
    static let amber = Color(hex: "#FFB020")
    static let success = Color(hex: "#22C55E")
    static let danger = Color(hex: "#FF4D5E")
    /// Conversa no padrão do Wabi.
    static let myBubble = Color.adaptive(light: "#111113", dark: "#F2F2F0")
    static let myBubbleText = Color.adaptive(light: "#FFFFFF", dark: "#111113")
    /// Read ✓✓: blue that holds on both my bubble (black light, white dark) and the list.
    static let receiptRead = Color.adaptive(light: "#53BDEB", dark: "#1F8FD8")
    static let otherBubble = Color.adaptive(light: "#F2F2F2", dark: "#26272B")
    /// Over a chat background: opaque white / lifted slate for contrast.
    static let otherBubbleOnBackdrop = Color.adaptive(light: "#FFFFFF", dark: "#2E2F34")

    static var actionGradient: LinearGradient {
        LinearGradient(colors: [action, actionDeep], startPoint: .topLeading, endPoint: .bottomTrailing)
    }
}

struct ActionGlassForeground: ShapeStyle {
    func resolve(in environment: EnvironmentValues) -> Color {
        // The prominent fill stays the action colour in inactive windows too, so its ink must as well.
        Palette.onAction
    }
}

// MARK: - Fundo com profundidade (o vidro precisa de algo por baixo)

struct NightBackdrop: View {
    @Environment(\.colorScheme) private var scheme
    var body: some View {
        ZStack {
            Palette.background
            MeshGradient(
                width: 3, height: 3,
                points: [[0, 0], [0.5, 0], [1, 0], [0, 0.5], [0.55, 0.45], [1, 0.5], [0, 1], [0.5, 1], [1, 1]],
                colors: scheme == .dark
                    ? [Color(hex: "#132048"), Color(hex: "#0B1122"), Color(hex: "#24154A"),
                       Color(hex: "#090D15"), Color(hex: "#0C1426"), Color(hex: "#0A0F1C"),
                       Color(hex: "#090D15"), Color(hex: "#090D15"), Color(hex: "#090D15")]
                    : [Color(hex: "#DCE6FF"), Color(hex: "#F1F4FC"), Color(hex: "#ECE3FF"),
                       Color(hex: "#F4F6FB"), Color(hex: "#F4F6FB"), Color(hex: "#F4F6FB"),
                       Color(hex: "#F4F6FB"), Color(hex: "#F4F6FB"), Color(hex: "#F4F6FB")]
            )
            .opacity(0.9)
        }
        .ignoresSafeArea()
    }
}

// MARK: - Tipografia

extension Font {
    /// Momento editorial (títulos de Itens): serifada do sistema (New York).
    static func editorial(_ size: CGFloat, weight: Font.Weight = .semibold) -> Font {
        .system(size: size, weight: weight, design: .serif)
    }
}

// MARK: - Tempo

enum MessageTime {
    static func short(_ ms: Int64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(ms) / 1000)
        let cal = Calendar.current
        if cal.isDateInToday(date) { return date.formatted(.dateTime.hour().minute()) }
        if cal.isDateInYesterday(date) { return String(localized: "Yesterday") }
        if let days = cal.dateComponents([.day], from: date, to: .now).day, days < 7 {
            return date.formatted(.dateTime.weekday(.abbreviated)).capitalized.replacingOccurrences(of: ".", with: "")
        }
        return date.formatted(.dateTime.day().month(.abbreviated))
    }

    /// Clock time only, as inside a bubble (day headers carry the date).
    static func clock(_ ms: Int64) -> String {
        Date(timeIntervalSince1970: TimeInterval(ms) / 1000).formatted(.dateTime.hour().minute())
    }

    static func relative(_ ms: Int64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(ms) / 1000)
        let secs = Date.now.timeIntervalSince(date)
        if secs < 60 { return String(localized: "now") }
        if secs < 3600 { return "\(Int(secs / 60)) min" }
        if secs < 86_400 { return "\(Int(secs / 3600)) h" }
        return short(ms)
    }
}

// MARK: - Superfícies

struct SectionHeader: View {
    let title: String
    var trailing: String? = nil
    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(title)
                .font(.footnote.weight(.semibold))
                .textCase(.uppercase)
                .tracking(0.6)
                .foregroundStyle(Palette.textSecondary)
            Spacer()
            if let trailing {
                Text(trailing).font(.footnote).foregroundStyle(Palette.textTertiary)
            }
        }
        .padding(.horizontal, 4)
    }
}
