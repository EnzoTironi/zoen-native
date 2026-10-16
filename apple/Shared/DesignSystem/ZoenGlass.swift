import SwiftUI

/// Zoen's one material: a marble of glass. Every button (GlassButtonStyle) and every card
/// (`.glassCard()`) is drawn by this modifier, so the look lives in one place.
///
/// Two layers: the lens underneath bends whatever is behind it, strongest at the curved
/// edge (Liquid Glass for controls; a material for cards, which stay cheap in scrolling
/// lists), and ZoenGlass.metal on top paints the volume: the shoulder's hemisphere
/// shading, specular, rim and inner shadow measured from Wabi's lens (VIDEO-ANALYSIS.md).
struct ZoenGlass: ViewModifier {
    var radius: CGFloat
    var prominent = false
    var tint: Color? = nil
    var pressed = false
    /// Controls get the real refracting lens; content (cards) gets a material.
    var lens = true
    /// How much volume to paint: 1 for controls, less for big content surfaces.
    var strength: CGFloat = 1
    /// Wabi's two lens states: 0 clear (sharp specular, thin film) … 1 frosted. Press clears it.
    var frost: CGFloat = 0.35
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorScheme) private var scheme

    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        content
            .background {
                ZStack {
                    if reduceTransparency {
                        shape.fill(prominent ? fill : AnyShapeStyle(Palette.surface))
                    } else if lens {
                        Color.clear.glassEffect(prominent ? .regular.tint(tintColor.opacity(0.88)).interactive() : .regular.interactive(), in: shape)
                    } else {
                        shape.fill(.ultraThinMaterial)
                        shape.fill(prominent ? fill : AnyShapeStyle(Color.white.opacity(scheme == .dark ? 0.05 : 0.45)))
                            .opacity(prominent ? 0.88 : 1)
                    }
                    GlassVolume(radius: radius, rimAngle: pressed ? -125 : -75, shine: pressed ? 0.25 : 0,
                                press: pressed ? 1 : 0, prominent: prominent, strength: strength, frost: frost)
                }
                // the marble's contact shadow and its long soft drop, tinted by the glass
                .shadow(color: .black.opacity(pressed ? 0.16 : 0.10), radius: pressed ? 1 : 2, y: pressed ? 1 : 3)
                .shadow(color: (prominent ? tintColor : .black).opacity(pressed ? 0.08 : 0.14 * strength),
                        radius: pressed ? 6 : 16, y: pressed ? 4 : 10)
            }
    }

    private var tintColor: Color { tint ?? Palette.action }
    private var fill: AnyShapeStyle { tint.map { AnyShapeStyle($0) } ?? AnyShapeStyle(.tint) }
}

extension View {
    /// The marble material in any rounded shape (Capsule = radius ≥ half the height).
    func zoenGlass(radius: CGFloat = 999, prominent: Bool = false, tint: Color? = nil, pressed: Bool = false,
                   lens: Bool = true, strength: CGFloat = 1, frost: CGFloat = 0.35) -> some View {
        modifier(ZoenGlass(radius: radius, prominent: prominent, tint: tint, pressed: pressed, lens: lens, strength: strength, frost: frost))
    }

    /// Cards (items, widgets, sheets' cards): padded content on the marble.
    func glassCard(radius: CGFloat = 22, padding: CGFloat = 16) -> some View {
        self.padding(padding).zoenGlass(radius: radius, lens: false, strength: 0.7, frost: 0.8)
    }
}

/// Runs the Metal shader; animatable so the rim turns, the shine slides and the highlight sinks on press.
private struct GlassVolume: View, Animatable {
    var radius: CGFloat
    var rimAngle: Double
    var shine: Double
    var press: Double
    var prominent: Bool
    var strength: CGFloat
    var frost: CGFloat

    var animatableData: AnimatablePair<AnimatablePair<Double, Double>, Double> {
        get { .init(.init(rimAngle, shine), press) }
        set { rimAngle = newValue.first.first; shine = newValue.first.second; press = newValue.second }
    }

    var body: some View {
        GeometryReader { geo in
            Rectangle()
                .fill(.white)
                .colorEffect(ShaderLibrary.zoenGlass(.float2(geo.size), .float(min(radius, geo.size.height / 2)),
                                                     .float(rimAngle), .float(shine), .float(press),
                                                     .float(prominent ? 1 : 0), .float(strength),
                                                     .float(frost * (1 - press))))
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}
