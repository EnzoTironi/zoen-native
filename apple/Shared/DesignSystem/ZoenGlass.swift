import SwiftUI

/// Zoen's one material: a marble of glass. Every button (GlassButtonStyle) and every card
/// (`.glassCard()`) is drawn by this modifier, so the look lives in one place.
///
/// Two layers: the lens underneath bends whatever is behind it, strongest at the curved
/// rim (Liquid Glass for controls; a material for cards, which stay cheap in scrolling
/// lists), and ZoenGlass.metal on top paints the optics measured in Wabi's app: Fresnel,
/// the 27°/65° key light, frost and the dispersion fringe on the rail.
struct ZoenGlass: ViewModifier {
    var radius: CGFloat
    var prominent = false
    var tint: Color? = nil
    var pressed = false
    /// Controls get the real refracting lens; content (cards) gets a material.
    var lens = true
    /// How much light to paint: 1 for controls, less for big content surfaces.
    var strength: CGFloat = 1
    /// Wabi's flat button preset (thin rail, frost .2, no dispersion) for small controls;
    /// false is the full marble for hero buttons and cards.
    var slab = false
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
                        if prominent, tint == nil {
                            // no explicit tint: the environment's .tint(...) fills the body, the lens bends it
                            shape.fill(.tint).opacity(0.88)
                            Color.clear.glassEffect(.regular.interactive(), in: shape)
                        } else {
                            Color.clear.glassEffect(prominent ? .regular.tint(tintColor.opacity(0.88)).interactive() : .regular.interactive(), in: shape)
                        }
                    } else {
                        shape.fill(.ultraThinMaterial)
                        shape.fill(prominent ? fill : AnyShapeStyle(Color.white.opacity(scheme == .dark ? 0.05 : 0.45)))
                            .opacity(prominent ? 0.88 : 1)
                    }
                    if !reduceTransparency {
                        GlassOptics(radius: radius, press: pressed ? 1 : 0, prominent: prominent, strength: strength, slab: slab)
                    }
                }
                // Wabi's drop: 26% black, long and soft, pulled in (spread -8); scaled to the control
                .shadow(color: (prominent ? tintColor : .black).opacity(pressed ? 0.12 : (slab ? 0.14 : 0.26) * strength),
                        radius: pressed ? 6 : (slab ? 8 : 18), x: slab ? 0 : 1, y: pressed ? 3 : (slab ? 4 : 12))
            }
    }

    private var tintColor: Color { tint ?? Palette.action }   // shadow colour when the tint comes from the environment
    private var fill: AnyShapeStyle { tint.map { AnyShapeStyle($0) } ?? AnyShapeStyle(.tint) }
}

extension View {
    /// The marble material in any rounded shape (a capsule when radius ≥ half the height).
    func zoenGlass(radius: CGFloat = 999, prominent: Bool = false, tint: Color? = nil, pressed: Bool = false,
                   lens: Bool = true, strength: CGFloat = 1, slab: Bool = false) -> some View {
        modifier(ZoenGlass(radius: radius, prominent: prominent, tint: tint, pressed: pressed, lens: lens, strength: strength, slab: slab))
    }

    /// Cards (items, widgets, sheets' cards): padded content on the marble.
    func glassCard(radius: CGFloat = 22, padding: CGFloat = 16) -> some View {
        self.padding(padding).zoenGlass(radius: radius, lens: false, strength: 0.8)
    }
}

/// Runs the Metal shader; animatable so the highlight slides and the frost clears on press.
private struct GlassOptics: View, Animatable {
    var radius: CGFloat
    var press: Double
    var prominent: Bool
    var strength: CGFloat
    var slab: Bool

    nonisolated var animatableData: Double {
        get { press }
        set { press = newValue }
    }

    var body: some View {
        let frost = (slab ? 0.2 : 0.6) * (1 - 0.5 * press)     // a press clears the lens
        GeometryReader { geo in
            Rectangle()
                .fill(.white)
                .colorEffect(ShaderLibrary.zoenGlass(.float2(geo.size), .float(min(radius, geo.size.height / 2)),
                                                     .float(press), .float(prominent ? 1 : 0), .float(strength),
                                                     .float(frost), .float(slab ? 1 : 0)))
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}
