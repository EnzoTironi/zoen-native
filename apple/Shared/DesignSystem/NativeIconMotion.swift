import SwiftUI

private struct NativeIconReduceMotionKey: EnvironmentKey {
    static let defaultValue = false
}
extension EnvironmentValues {
    /// App motion preference, combined with the system preference by native icons.
    var nativeIconReduceMotion: Bool {
        get { self[NativeIconReduceMotionKey.self] }
        set { self[NativeIconReduceMotionKey.self] = newValue }
    }
}

/// Effects belong to the actual SF Image, never a bitmap or its outer container.
struct NativeIconMotion: View {
    var systemName: String
    var size: CGFloat? = nil
    var bounce: Int = 0
    var wiggle: Int = 0
    var active = false
    var drawOn = false
    @Environment(\.accessibilityReduceMotion) private var systemReduce
    @Environment(\.nativeIconReduceMotion) private var appReduce
    @Environment(\.ambientPaused) private var paused
    private var reduced: Bool { systemReduce || appReduce }

    var body: some View {
        // Keep the semantic SF Image alive even in native button-label extraction.
        // DrawOn is a native indefinite effect: activation reveals it once and
        // retains the completed glyph, without an empty first-appearance branch.
        Group {
            if drawOn && !reduced && !paused {
                artwork.symbolEffect(.drawOn, options: .nonRepeating, isActive: true)
            } else {
                // Inactive drawOn represents the drawn-off state, not a static icon.
                artwork
            }
        }
            .animation(reduced || paused ? nil : .smooth(duration: 0.2), value: systemName)
            .transaction { if reduced || paused { $0.animation = nil } }
            .accessibilityHidden(true)
    }

    @ViewBuilder private var artwork: some View {
        if #available(iOS 26.0, macOS 26.0, *) {
            image.contentTransition(reduced ? .opacity : .symbolEffect(.replace.magic(fallback: .replace)))
        } else {
            image.contentTransition(reduced ? .opacity : .symbolEffect(.replace))
        }
    }

    private var image: some View {
        Image(systemName: systemName)
            .modifier(NativeIconFont(size: size))
            .symbolEffect(.bounce, options: .speed(1.5), value: bounce)
            .symbolEffect(.wiggle, value: wiggle)
            .symbolEffect(.variableColor.iterative, isActive: active && !reduced && !paused)
            // Keep event identities stable across preference changes; never replay taps.
            .symbolEffectsRemoved(reduced || paused)
    }
}

private struct NativeIconFont: ViewModifier {
    var size: CGFloat?
    @ViewBuilder func body(content: Content) -> some View {
        if let size { content.font(.system(size: size)) } else { content }
    }
}

/// Bitmap avatars use container press feedback, not SF symbol effects.
struct NativeIconPressStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var systemReduce
    @Environment(\.nativeIconReduceMotion) private var appReduce
    @Environment(\.isEnabled) private var enabled
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed && enabled && !systemReduce && !appReduce ? 0.96 : 1)
            .animation(.easeOut(duration: 0.15), value: configuration.isPressed)
    }
}
