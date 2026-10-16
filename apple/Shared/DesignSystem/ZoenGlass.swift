import SwiftUI

/// One rounded lens for buttons and cards. Only the material is sampled: labels stay sharp.
struct ZoenGlass: ViewModifier {
    var radius: CGFloat
    var prominent = false
    var tint: Color? = nil
    var pressed = false
    var lens = true
    var strength: CGFloat = 1
    var slab = false
    var appearance: ColorScheme? = nil
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorScheme) private var scheme

    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        content.background {
            ZStack {
                if reduceTransparency {
                    // Composite translucent tints over a truly opaque, appearance-local surface.
                    shape.fill((appearance ?? scheme) == .dark ? Color.black : Color.white)
                    if prominent { shape.fill(fill) }
                } else {
                    GlassOptics(radius: radius, press: pressed ? 1 : 0,
                                strength: strength, slab: slab, fill: fill,
                                prominent: prominent, dark: (appearance ?? scheme) == .dark)
                        .environment(\.colorScheme, appearance ?? scheme)
                }
            }
            .background {
                // A separate inset silhouette implements negative spread without shrinking the lens.
                shape.inset(by: slab ? 0 : 8)
                    .fill(.black.opacity(pressed ? 0.12 : 0.26))
                    .blur(radius: pressed ? 6 : (slab ? 8 : 60))
                    .offset(x: slab ? 0 : 2, y: pressed ? 3 : (slab ? 4 : 41))
            }
        }
    }
    private var fill: AnyShapeStyle { tint.map { AnyShapeStyle($0) } ?? AnyShapeStyle(.tint) }
}

extension View {
    @ViewBuilder
    func zoenGlass(radius: CGFloat = 999, prominent: Bool = false, tint: Color? = nil, pressed: Bool = false,
                   lens: Bool = true, strength: CGFloat = 1, slab: Bool = false, enabled: Bool = true,
                   appearance: ColorScheme? = nil) -> some View {
        if enabled {
            modifier(ZoenGlass(radius: radius, prominent: prominent, tint: tint, pressed: pressed,
                               lens: lens, strength: strength, slab: slab, appearance: appearance))
        } else {
            self
        }
    }
    func glassCard(radius: CGFloat = 22, padding: CGFloat = 16) -> some View {
        self.padding(padding).zoenGlass(radius: radius, lens: false, strength: 0.8)
    }
}

private struct GlassOptics: View, Animatable {
    var radius: CGFloat
    var press: Double
    var strength: CGFloat
    var slab: Bool
    var fill: AnyShapeStyle
    var prominent: Bool
    var dark: Bool
    nonisolated var animatableData: Double {
        get { press }
        set { press = newValue }
    }
    var body: some View {
        GeometryReader { geo in
            let r = min(radius, min(geo.size.width, geo.size.height) / 2)
            let shape = RoundedRectangle(cornerRadius: r, style: .continuous)
            ZStack {
                // System materials cannot be sampled reliably by SwiftUI::Layer.
                // Sample a local translucent field; keep the live backdrop outside the shader.
                if prominent {
                    shape.fill(fill)
                } else {
                    shape.fill(.white.opacity(dark ? 0.08 : 0.58))
                }
                // A low-contrast field makes lens bending visible even without a busy backdrop.
                shape.fill(LinearGradient(colors: [.white.opacity(0.08), .clear, .black.opacity(0.08)],
                                          startPoint: .topLeading, endPoint: .bottomTrailing))
            }
            .compositingGroup()
            .layerEffect(ShaderLibrary.zoenGlass(.float2(geo.size), .float(r), .float(press),
                                               .float(prominent ? 1 : 0), .float(strength),
                                               .float(0), .float(slab ? 1 : 0)),
                         maxSampleOffset: CGSize(width: r, height: r))
            .background { shape.fill(.ultraThinMaterial) }
            .clipShape(shape)
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}
