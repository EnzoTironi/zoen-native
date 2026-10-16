import SwiftUI

/// Shared sizing and typography, with the platform's native button interaction and material.
struct GlassButtonStyle: PrimitiveButtonStyle {
    var prominent = false
    var tint: Color? = nil
    var fullWidth = false
    var height: CGFloat = 46
    var font: Font = .headline

    func makeBody(configuration: Configuration) -> some View {
        NativeGlassButton(configuration: configuration, prominent: prominent, tint: tint,
                          fullWidth: fullWidth, height: height, font: font)
    }
}

extension PrimitiveButtonStyle where Self == GlassButtonStyle {
    static var wabiGlass: GlassButtonStyle { GlassButtonStyle() }
    static var wabiGlassProminent: GlassButtonStyle { GlassButtonStyle(prominent: true) }
    static func wabiGlass(prominent: Bool = false, tint: Color? = nil, fullWidth: Bool = false, height: CGFloat = 46) -> GlassButtonStyle {
        GlassButtonStyle(prominent: prominent, tint: tint, fullWidth: fullWidth, height: height)
    }
}

private struct NativeGlassButton: View {
    let configuration: PrimitiveButtonStyleConfiguration
    let prominent: Bool
    let tint: Color?
    let fullWidth: Bool
    let height: CGFloat
    let font: Font
    @Environment(\.controlSize) private var controlSize

    var body: some View {
        // Reuse the configuration so the native style owns activation, disabled state,
        // accessibility and press feedback, rather than simulating a second button.
        styledButton
            .modifier(OptionalGlassTint(color: tint))
    }

    @ViewBuilder private var styledButton: some View {
        if prominent {
            button.buttonStyle(.glassProminent)
        } else {
            button.buttonStyle(.glass)
        }
    }

    @ViewBuilder private var button: some View {
        if fullWidth {
            sizedButton.buttonSizing(.flexible)
        } else {
            sizedButton
        }
    }

    private var sizedButton: some View {
        SwiftUI.Button(action: configuration.trigger) {
            configuration.label
                .font(font)
                .frame(maxWidth: fullWidth ? .infinity : nil,
                       minHeight: controlSize == .small || controlSize == .mini ? min(height, 34) : (controlSize == .large || controlSize == .extraLarge ? max(height, 52) : height))
        }
    }
}

private struct OptionalGlassTint: ViewModifier {
    let color: Color?
    @ViewBuilder func body(content: Content) -> some View {
        if let color { content.tint(color) } else { content }
    }
}

#Preview("Native glass") {
    GlassEffectContainer(spacing: 16) {
        VStack(spacing: 16) {
            Button("Hi, Zoen") {}.buttonStyle(.wabiGlass(prominent: true, fullWidth: true, height: 56))
            Button("Continue") {}.buttonStyle(.wabiGlass)
            Button("Disabled") {}.buttonStyle(.wabiGlassProminent).disabled(true)
        }
    }
    .padding(24)
    .background(InkPalette.paper)
}
