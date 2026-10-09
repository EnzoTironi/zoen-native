import SwiftUI

// MARK: - Bottom sheets: how they land, snap and grow
//
// The system sheet already gives the native feel we want and we keep it: an interruptible
// spring, rubber-banding past the top detent, velocity-aware snapping, and a backdrop that
// dims with the drag. On top of that, every Zoen sheet:
// - ticks lightly when it snaps to another detent (not when it first appears);
// - lets its content land a beat after the sheet does: a short fade with a slight rise and
//   scale, staggered row by row (`sheetItem`). Reduce Motion: a plain fade, no stagger;
// - can size itself to its content (`zoenFittedSheet`): the detent follows the content as
//   it grows (a spring, not a jump), quantized so typing doesn't make it wobble.

enum SheetMotion {
    /// iOS-like spring for things inside sheets (response 0.4, damping 0.86).
    static let spring = Animation.spring(response: 0.4, dampingFraction: 0.86)
    /// The sheet's own rise takes about this long; content waits for it.
    static let landDelay: Double = 0.14
    static let stagger: Double = 0.045
}

extension EnvironmentValues {
    /// False while a sheet is still rising; `sheetItem` content waits for it.
    @Entry var sheetLanded: Bool = true
}

extension View {
    /// Detents with a light tick on each snap, the drag indicator, and landing content.
    func zoenSheet(_ detents: [PresentationDetent], initial: PresentationDetent? = nil) -> some View {
        modifier(ZoenSheetModifier(detents: detents, initial: initial ?? detents.first ?? .large))
    }

    /// A sheet as tall as its content (between `min` and `max`), following it as it grows.
    func zoenFittedSheet(min: CGFloat = 200, max: CGFloat = 640, extra: [PresentationDetent] = []) -> some View {
        modifier(FittedSheetModifier(minHeight: min, maxHeight: max, extra: extra))
    }

    /// One piece of a sheet's content: it lands after the sheet, `index` steps behind the first.
    func sheetItem(_ index: Int = 0) -> some View { modifier(SheetItemModifier(index: index)) }
}

private struct LandingModifier: ViewModifier {
    @State private var landed = false
    func body(content: Content) -> some View {
        content
            .environment(\.sheetLanded, landed)
            .task {
                try? await Task.sleep(for: .seconds(SheetMotion.landDelay))
                landed = true
            }
    }
}

private struct ZoenSheetModifier: ViewModifier {
    let detents: [PresentationDetent]
    let initial: PresentationDetent
    @State private var selected: PresentationDetent?

    func body(content: Content) -> some View {
        content
            .modifier(LandingModifier())
            .presentationDetents(Set(detents), selection: Binding(get: { selected ?? initial }, set: { selected = $0 }))
            .presentationDragIndicator(detents.count > 1 ? .visible : .automatic)
            .onChange(of: selected) { old, new in
                if old != nil, new != nil, old != new { Haptics.selectionTick() }
            }
    }
}

private struct FittedSheetModifier: ViewModifier {
    let minHeight: CGFloat
    let maxHeight: CGFloat
    let extra: [PresentationDetent]
    @State private var height: CGFloat = 0
    @State private var selected: PresentationDetent?

    private var fitted: PresentationDetent { .height(Swift.min(maxHeight, Swift.max(minHeight, height))) }

    func body(content: Content) -> some View {
        content
            .fixedSize(horizontal: false, vertical: true)
            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { h in
                // Round up to 8 pt so a line of typing doesn't nudge it every keystroke.
                let q = (h / 8).rounded(.up) * 8 + 24
                guard abs(q - height) >= 8 else { return }
                if height == 0 { height = q } else { withAnimation(SheetMotion.spring) { height = q } }
            }
            .frame(maxHeight: .infinity, alignment: .top)
            .modifier(LandingModifier())
            .presentationDetents(Set([fitted] + extra), selection: Binding(get: { selected ?? fitted }, set: { selected = $0 }))
            .presentationDragIndicator(extra.isEmpty ? .hidden : .visible)
            .onChange(of: height) { _, _ in if !extra.contains(selected ?? fitted) { selected = fitted } }
            .onChange(of: selected) { old, new in
                if old != nil, new != nil, old != new, extra.contains(new!) || extra.contains(old!) { Haptics.selectionTick() }
            }
    }
}

private struct SheetItemModifier: ViewModifier {
    let index: Int
    @Environment(\.sheetLanded) private var landed
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func body(content: Content) -> some View {
        content
            .opacity(landed ? 1 : 0)
            .scaleEffect(landed || reduceMotion ? 1 : 0.985, anchor: .top)
            .offset(y: landed || reduceMotion ? 0 : 10)
            .animation(reduceMotion ? .easeOut(duration: 0.2)
                       : SheetMotion.spring.delay(Double(min(index, 8)) * SheetMotion.stagger),
                       value: landed)
    }
}

/// For sheets that manage their own detents: just the landing beat for `sheetItem` content.
struct SheetLandingHost: ViewModifier {
    func body(content: Content) -> some View { content.modifier(LandingModifier()) }
}
