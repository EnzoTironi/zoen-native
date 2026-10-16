import SwiftUI

/// Zoen's one button: Wabi's press on the marble glass (see ZoenGlass.swift).
///
/// `.glass` is the clear pill for secondary actions; `.glassProminent` is the same lens
/// tinted with the brand green (or `tint:`) for the primary action on a screen.
struct GlassButtonStyle: ButtonStyle {
    var prominent = false
    var tint: Color? = nil
    var fullWidth = false
    var height: CGFloat = 46
    var font: Font = .headline

    func makeBody(configuration: Configuration) -> some View {
        GlassButtonBody(configuration: configuration, prominent: prominent, tint: tint,
                        fullWidth: fullWidth, height: height, font: font)
    }
}

extension ButtonStyle where Self == GlassButtonStyle {
    static var wabiGlass: GlassButtonStyle { GlassButtonStyle() }
    static var wabiGlassProminent: GlassButtonStyle { GlassButtonStyle(prominent: true) }
    static func wabiGlass(prominent: Bool = false, tint: Color? = nil, fullWidth: Bool = false, height: CGFloat = 46) -> GlassButtonStyle {
        GlassButtonStyle(prominent: prominent, tint: tint, fullWidth: fullWidth, height: height)
    }
}

private struct GlassButtonBody: View {
    let configuration: ButtonStyleConfiguration
    let prominent: Bool
    let tint: Color?
    let fullWidth: Bool
    let height: CGFloat
    let font: Font
    @Environment(\.isEnabled) private var isEnabled
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.controlSize) private var controlSize

    var body: some View {
        let pressed = configuration.isPressed && isEnabled
        configuration.label
            .font(font)
            .foregroundStyle(prominent ? Color.white : Palette.textPrimary)
            .shadow(color: .black.opacity(0.05), radius: 0.5, y: 0.8)      // text-shadow 0 .05em .05em
            .padding(.horizontal, controlSize == .small || controlSize == .mini ? 14 : 24)
            .frame(maxWidth: fullWidth ? .infinity : nil, minHeight: controlSize == .small || controlSize == .mini ? min(height, 34) : (controlSize == .large || controlSize == .extraLarge ? max(height, 52) : height))
            .zoenGlass(prominent: prominent, tint: tint, pressed: pressed)
            .contentShape(.capsule)
            // Wabi: scale(.975) and rotate3d(1,0,0,25deg) while held; better-ui press is 0.96.
            .scaleEffect(pressed && !reduceMotion ? 0.96 : 1)
            .rotation3DEffect(.degrees(pressed && !reduceMotion ? 10 : 0), axis: (1, 0, 0), perspective: 0.5)
            .opacity(isEnabled ? 1 : (pressed ? 0.8 : 0.45))
            .animation(reduceMotion ? .easeOut(duration: 0.15) : .spring(duration: 0.35, bounce: 0.4), value: pressed)
            .animation(.easeOut(duration: 0.15), value: isEnabled)
            .sensoryFeedback(.impact(weight: .light, intensity: 0.6), trigger: pressed) { _, now in now }
    }
}

#Preview("Wabi glass") {
    VStack(spacing: 16) {
        Button("Hi, Zoen") {}.buttonStyle(.wabiGlass(prominent: true, fullWidth: true, height: 56))
        Button("Continue") {}.buttonStyle(.wabiGlass)
        Button("Disabled") {}.buttonStyle(.wabiGlassProminent).disabled(true)
    }
    .padding(24)
    .background(InkPalette.paper)
}
