import SwiftUI

/// Roda's brand shimmer: moss green → lime → blush pink (the mascot's colors), sliding.
enum BrandShimmer {
    static let colors = [Color(hex: "#4F8A2B"), Color(hex: "#A6D05A"), Color(hex: "#F49A9A"), Color(hex: "#4F8A2B")]
    static func gradient(phase: CGFloat) -> LinearGradient {
        LinearGradient(colors: colors, startPoint: UnitPoint(x: -1 + 2 * phase, y: 0.2), endPoint: UnitPoint(x: 2 * phase, y: 0.8))
    }
}

/// The agent-thinking state: a container that grows from a small spark into a capsule
/// with a shimmering brand border and text, and resizes smoothly as the phrase changes.
struct ThinkingShimmer: View {
    let phrases: [String]
    var compact = false
    var showsHead = true            // Zoen's face; off when an avatar already sits beside it
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var expanded = false
    @State private var start = Date()

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 30, paused: reduceMotion)) { tl in
            let t = reduceMotion ? 0 : tl.date.timeIntervalSince(start)
            let phase = CGFloat((t * 0.55).truncatingRemainder(dividingBy: 1))
            let index = phrases.isEmpty ? 0 : min(phrases.count - 1, Int(t / 1.9))
            HStack(spacing: 10) {
                if showsHead { MascotHead(size: compact ? 22 : 28) }
                if expanded, !phrases.isEmpty {
                    Text(phrases[index])
                        .font(compact ? .caption.weight(.medium) : .subheadline.weight(.semibold))
                        .foregroundStyle(reduceMotion ? AnyShapeStyle(Palette.textSecondary) : AnyShapeStyle(LinearGradient(
                            colors: [Palette.textSecondary, Color(hex: "#4F8A2B"), Color(hex: "#E27D7D"), Palette.textSecondary],
                            startPoint: UnitPoint(x: -1 + 2.2 * phase, y: 0.5), endPoint: UnitPoint(x: 2.2 * phase, y: 0.5))))
                        .lineLimit(1)
                        .id(index)
                        .transition(.asymmetric(insertion: .opacity.combined(with: .offset(y: 6)), removal: .opacity.combined(with: .offset(y: -6))))
                }
            }
            .padding(.leading, showsHead ? (compact ? 6 : 8) : (compact ? 12 : 16))
            .padding(.trailing, expanded ? 16 : (compact ? 6 : 8))
            .padding(.vertical, compact ? 6 : 8)
            .background {
                Capsule().fill(Palette.surfaceRaised)
                Capsule().fill(BrandShimmer.gradient(phase: phase)).opacity(0.12)
            }
            .overlay(Capsule().strokeBorder(BrandShimmer.gradient(phase: phase), lineWidth: 1.6))
            .animation(.spring(duration: 0.5, bounce: 0.25), value: index)
        }
        .onAppear {
            start = .now
            withAnimation(reduceMotion ? nil : .spring(duration: 0.55, bounce: 0.3).delay(0.08)) { expanded = true }
        }
        .accessibilityElement(children: .combine)
    }
}
