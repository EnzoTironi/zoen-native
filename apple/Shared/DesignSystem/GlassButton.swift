import SwiftUI

/// Zoen's one button material: Wabi's glass (see WabiGlass.metal).
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
            .background { GlassSurface(prominent: prominent, tint: tint, pressed: pressed) }
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

/// The material on its own, for controls that aren't Buttons (menus, chips, pickers).
/// With no `tint`, the prominent lens takes the environment's `.tint(...)`.
struct GlassSurface: View {
    var prominent = false
    var tint: Color? = nil
    var pressed = false
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorScheme) private var scheme

    private var fill: AnyShapeStyle { tint.map { AnyShapeStyle($0) } ?? AnyShapeStyle(.tint) }

    var body: some View {
        let capsule = Capsule(style: .continuous)
        ZStack {
            if reduceTransparency {
                if prominent { capsule.fill(fill) } else { capsule.fill(Palette.surface) }
            } else {
                capsule.fill(.ultraThinMaterial)
                if prominent { capsule.fill(fill).opacity(0.86) } else { capsule.fill(Color.white.opacity(scheme == .dark ? 0.06 : 0.35)) }
            }
            GlassSheen(rimAngle: pressed ? -125 : -75, shine: pressed ? 0.25 : 0, prominent: prominent)
        }
        // 0 .25em .125em -.125em rgba(0,0,0,.1) and the header pill's long soft drop
        .shadow(color: .black.opacity(pressed ? 0.16 : 0.10), radius: pressed ? 1 : 2, y: pressed ? 1 : 3)
        .shadow(color: (prominent ? (tint ?? .black) : .black).opacity(pressed ? 0.06 : 0.12), radius: pressed ? 6 : 14, y: pressed ? 4 : 10)
    }
}

/// Runs the Metal shader; animatable so the rim turns and the shine slides on press.
private struct GlassSheen: View, Animatable {
    var rimAngle: Double
    var shine: Double
    var prominent: Bool

    var animatableData: AnimatablePair<Double, Double> {
        get { .init(rimAngle, shine) }
        set { rimAngle = newValue.first; shine = newValue.second }
    }

    var body: some View {
        GeometryReader { geo in
            Capsule(style: .continuous)
                .fill(.white)
                .colorEffect(ShaderLibrary.wabiGlass(.float2(geo.size), .float(rimAngle), .float(shine), .float(prominent ? 1 : 0)))
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
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
