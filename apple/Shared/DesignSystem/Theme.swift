import SwiftUI
import RodaCore

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
    static func adaptive(
        light: String,
        dark: String,
        lightAlpha: Double = 1,
        darkAlpha: Double = 1,
        contrastLightAlpha: Double? = nil,
        contrastDarkAlpha: Double? = nil
    ) -> Color {
        #if canImport(UIKit)
        Color(UIColor { traits in
            let night = traits.userInterfaceStyle == .dark
            let contrast = traits.accessibilityContrast == .high
            let alpha = night
                ? (contrast ? (contrastDarkAlpha ?? darkAlpha) : darkAlpha)
                : (contrast ? (contrastLightAlpha ?? lightAlpha) : lightAlpha)
            return UIColor(hex: night ? dark : light, alpha: CGFloat(alpha))
        })
        #else
        Color(NSColor(name: nil) { appearance in
            let night = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            let contrast = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast
            let alpha = night
                ? (contrast ? (contrastDarkAlpha ?? darkAlpha) : darkAlpha)
                : (contrast ? (contrastLightAlpha ?? lightAlpha) : lightAlpha)
            return NSColor(hex: night ? dark : light, alpha: CGFloat(alpha))
        })
        #endif
    }
}

extension PlatformColor {
    convenience init(hex: String, alpha: CGFloat = 1) {
        var s = hex.trimmingCharacters(in: .whitespacesAndNewlines)
        if s.hasPrefix("#") { s.removeFirst() }
        var v: UInt64 = 0
        Scanner(string: s).scanHexInt64(&v)
        let r = CGFloat((v >> 16) & 0xFF) / 255, g = CGFloat((v >> 8) & 0xFF) / 255, b = CGFloat(v & 0xFF) / 255
        self.init(red: r, green: g, blue: b, alpha: alpha)
    }
}

enum Palette {
    static let background = Color.adaptive(light: "#FFFFFF", dark: "#000000")
    static let textPrimary = Color.adaptive(light: "#1D1D1F", dark: "#F5F5F7")
    static let textSecondary = Color.adaptive(light: "#1D1D1F", dark: "#F5F5F7", lightAlpha: 0.5, darkAlpha: 0.5, contrastLightAlpha: 0.7, contrastDarkAlpha: 0.7)
    static let textTertiary = Color.adaptive(light: "#1D1D1F", dark: "#F5F5F7", lightAlpha: 0.3, darkAlpha: 0.3)
    static let fillPrimary = Color.adaptive(light: "#1D1D1F", dark: "#F5F5F7")
    static let onAction = Color.adaptive(light: "#F9F9FA", dark: "#1D1D1F")
    static let thinBg = Color.adaptive(light: "#FFFFFF", dark: "#1D1D1F", lightAlpha: 0.55, darkAlpha: 0.615)
    static let regularBg = Color.adaptive(light: "#FFFFFF", dark: "#1D1D1F", lightAlpha: 0.668, darkAlpha: 0.618)
    static let composerBg = Color.adaptive(light: "#FFFFFF", dark: "#1D1D1F", lightAlpha: 0.55, darkAlpha: 0.618)
    static let bubbleAgent = Color.adaptive(light: "#FFFFFF", dark: "#1D1D1F", lightAlpha: 0.98, darkAlpha: 0.7)
    static let bubbleUser = Color.adaptive(light: "#FFFFFF", dark: "#1D1D1F", lightAlpha: 0.3, darkAlpha: 0.3)
    static let cellBg = Color.adaptive(light: "#FFFFFF", dark: "#FFFFFF", lightAlpha: 0.6, darkAlpha: 0.05)
    static let strokePrimary = Color.adaptive(light: "#1D1D1F", dark: "#F5F5F7", lightAlpha: 0.14, darkAlpha: 0.14, contrastLightAlpha: 0.3, contrastDarkAlpha: 0.3)
    static let strokeSecondary = Color.adaptive(light: "#1D1D1F", dark: "#F5F5F7", lightAlpha: 0.07, darkAlpha: 0.07)
    static let strokeComposer = Color.adaptive(light: "#F5F5F7", dark: "#1D1D1F", lightAlpha: 0.5, darkAlpha: 0.5)
    static let overlay = Color.adaptive(light: "#000000", dark: "#000000", lightAlpha: 0.6, darkAlpha: 0.6)
    static let focusRing = Color.adaptive(light: "#000000", dark: "#FFFFFF", lightAlpha: 0.5, darkAlpha: 0.5)
    static let rowValue = Color.adaptive(light: "#1D1D1F", dark: "#FFFFFF", lightAlpha: 0.5, darkAlpha: 0.5)
    static let avatarBlue = Color(hex: "#528AFF")
    static let surface = regularBg
    static let surfaceRaised = thinBg
    static let surfaceMuted = cellBg
    static let hairline = strokeSecondary
    static let action = fillPrimary
    static let actionDeep = fillPrimary
    static let amber = Color(hex: "#FF4D00")
    static let success = Color(hex: "#00A665")
    static let danger = Color.adaptive(light: "#DA2F35", dark: "#F1555B")
    static let myBubble = bubbleUser
    static let myBubbleText = textPrimary
    static let otherBubble = bubbleAgent
    static let otherBubbleOnBackdrop = bubbleAgent

    static var actionGradient: LinearGradient {
        LinearGradient(colors: [fillPrimary, fillPrimary], startPoint: .topLeading, endPoint: .bottomTrailing)
    }
}

struct ActionGlassForeground: ShapeStyle {
    func resolve(in environment: EnvironmentValues) -> Color {
        #if os(macOS)
        if environment.controlActiveState != .key { return Palette.textPrimary }
        #endif
        return Palette.onAction
    }
}

// MARK: - Fundo com profundidade (o vidro precisa de algo por baixo)

struct NightBackdrop: View {
    var body: some View { SkyBackground() }
}

// MARK: - Tipografia

extension Font {
    static func editorial(_ size: CGFloat, weight: Font.Weight = .semibold) -> Font {
        .system(size: size, weight: weight)
    }
}

// MARK: - Tempo

enum RodaTime {
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

/// Conteúdo é sólido (nunca vidro): cartões, mensagens, Itens.
struct SolidCard: ViewModifier {
    var radius: CGFloat = 24
    var padding: CGFloat = 16
    @Environment(\.colorScheme) private var scheme
    func body(content: Content) -> some View {
        content
            .padding(padding)
            .background(Palette.regularBg, in: Superellipse(radius: radius))
            .overlay {
                if scheme == .light {
                    Superellipse(radius: radius)
                        .stroke(
                            LinearGradient(
                                colors: [Color.white.opacity(0.9), Color(hex: "#1D1D1F").opacity(0.06)],
                                startPoint: .top,
                                endPoint: .bottom
                            ),
                            lineWidth: 1
                        )
                        .allowsHitTesting(false)
                }
            }
    }
}

extension View {
    func solidCard(radius: CGFloat = 24, padding: CGFloat = 16) -> some View {
        modifier(SolidCard(radius: radius, padding: padding))
    }
}

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
