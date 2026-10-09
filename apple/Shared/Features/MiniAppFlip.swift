import SwiftUI

// MARK: - Opening a mini-app: the tile flips over and grows into the app (Abode-style)

extension EnvironmentValues {
    /// Set while a mini-app is shown by the flip host: its own close buttons call this
    /// (instead of `dismiss`, which has no sheet to dismiss there) so it flips back.
    @Entry var miniAppClose: (() -> Void)? = nil
}

extension AppModel {
    /// A mini-app opened from a tile: where the tile was, and the tile itself (the card's front).
    struct AppFlip: Identifiable {
        let id = UUID()
        let itemId: String
        let from: CGRect
        let sourceKey: String
        let front: AnyView
    }
}

/// Full-screen host for a flipped-open mini-app. The card starts exactly over the tile
/// (front = the tile), turns over in 3D while it grows to the screen, and lands as the live
/// app (back). Closing runs it backwards into the tile. Reduce Motion: a crossfade.
struct MiniAppFlipHost: View {
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let flip: AppModel.AppFlip

    @State private var p: Double = 0
    @State private var live = false

    private var openSpring: Animation {
        reduceMotion ? .easeInOut(duration: 0.25) : .spring(response: 0.62, dampingFraction: 0.84)
    }
    private var closeSpring: Animation {
        reduceMotion ? .easeInOut(duration: 0.22) : .spring(response: 0.52, dampingFraction: 0.9)
    }

    var body: some View {
        GeometryReader { outer in
            let insets = outer.safeAreaInsets
            GeometryReader { g in
                FlipCard(p: p, from: flip.from, to: CGRect(origin: .zero, size: g.size), flat: reduceMotion,
                         front: flip.front.allowsHitTesting(false),
                         back: AppSheetHost(itemId: flip.itemId)
                            .padding(.top, insets.top)
                            .padding(.bottom, insets.bottom)
                            .background(Palette.background)
                            .allowsHitTesting(live))
            }
            .ignoresSafeArea()
        }
        .background {
            Color.black.opacity(reduceMotion ? 0 : 0.22 * min(1, max(0, p)))
                .ignoresSafeArea()
                .allowsHitTesting(false)
        }
        .environment(\.appZoom, nil)
        .environment(\.miniAppClose, close)
        .onAppear(perform: open)
        .accessibilityAddTraits(.isModal)
    }

    private func open() {
        Haptics.open()
        withAnimation(openSpring) { p = 1 } completion: { live = true }
    }

    private func close() {
        guard live else { return }
        live = false
        Haptics.dismiss()
        withAnimation(closeSpring) { p = 0 } completion: {
            if model.appFlip?.id == flip.id { model.appFlip = nil }
        }
    }
}

/// The card itself, re-laid out every frame from one animated progress so the face swap
/// happens exactly when it's edge-on.
struct FlipCard<Front: View, Back: View>: View, Animatable {
    var p: Double
    let from: CGRect
    let to: CGRect
    let flat: Bool
    let front: Front
    let back: Back

    nonisolated var animatableData: Double {
        get { p }
        set { p = newValue }
    }

    var body: some View {
        if flat {
            back
                .frame(width: to.width, height: to.height)
                .opacity(min(1, max(0, p)))
                .position(x: to.midX, y: to.midY)
        } else {
            let t = CGFloat(p)
            let w = max(1, from.width + (to.width - from.width) * t)
            let h = max(1, from.height + (to.height - from.height) * t)
            let x = from.midX + (to.midX - from.midX) * t
            let y = from.midY + (to.midY - from.midY) * t
            let angle = 180 * p
            let showBack = angle >= 90
            let corner = 24 + (54 - 24) * min(1, max(0, t))
            let clamped = min(1, max(0, p))
            // It lifts toward you mid-turn and the face dims as it goes edge-on: depth without blur.
            let lift = 1 + 0.05 * sin(Double.pi * clamped)
            let edgeShade = 0.28 * (1 - abs(cos(Double.pi * clamped)))
            ZStack {
                front
                    .frame(width: from.width, height: from.height)
                    .scaleEffect(min(w / from.width, h / from.height))
                    .frame(width: w, height: h)
                    .opacity(showBack ? 0 : 1)
                back
                    .frame(width: to.width, height: to.height)
                    .scaleEffect(w / to.width, anchor: .top)
                    .frame(width: w, height: h, alignment: .top)
                    .clipped()
                    // Pre-turned, so the full turn shows it the right way round.
                    .rotation3DEffect(.degrees(180), axis: (x: 0, y: 1, z: 0))
                    .opacity(showBack ? 1 : 0)
            }
            .frame(width: w, height: h)
            .overlay(Color.black.opacity(edgeShade).allowsHitTesting(false))
            .clipShape(.rect(cornerRadius: corner, style: .continuous))
            .scaleEffect(lift)
            .shadow(color: .black.opacity(0.25 * sin(Double.pi * min(1, max(0, p)))), radius: 24, y: 14)
            .rotation3DEffect(.degrees(angle), axis: (x: 0, y: 1, z: 0), perspective: 0.45)
            .position(x: x, y: y)
        }
    }
}
