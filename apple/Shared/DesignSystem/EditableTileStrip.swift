import SwiftUI

// MARK: - Jiggle edit mode for a strip of mini-app tiles (Home, chat pins)

/// A horizontal strip of tiles with an iOS-style edit mode:
/// - long-press a tile → every tile jiggles (each on its own phase), a minus badge appears
///   on each one and a "Concluir" bar slides in above the strip;
/// - in edit mode, touch-and-hold a beat then drag to reorder: the tile lifts, follows the
///   finger, and its neighbours reflow around it; let go and it settles;
/// - the minus badge asks before removing (confirmation dialog), then the gap closes;
/// - haptics on enter, pick-up, every slot crossed, drop and remove;
/// - Reduce Motion: no wobble (badges and reorder still work).
struct EditableTileStrip<Item: Identifiable, Tile: View>: View where Item.ID == String {
    let items: [Item]
    let tileWidth: CGFloat
    var spacing: CGFloat = 12
    var margin: CGFloat = 20
    /// Accessibility identifier prefix ("home-tile", "pin-tile").
    var idPrefix: String
    @Binding var editing: Bool
    let title: (Item) -> String
    let open: (Item) -> Void
    /// When set, a tap hands over the tile's on-screen frame and the tile itself, so the
    /// mini-app can flip open out of it (MiniAppFlipHost).
    var openFrom: ((Item, CGRect, AnyView) -> Void)? = nil
    let move: ([String]) -> Void
    let remove: (Item) -> Void
    /// Confirmation copy: title for the item, the message, and the destructive button.
    let removeTitle: (Item) -> String
    var removeMessage: LocalizedStringKey = "It stays in the chat; you can pin it again from there."
    var removeAction: LocalizedStringKey = "Unpin"
    /// Screenshots: show the last tile shortly after appearing.
    var scrollToEnd = false
    @ViewBuilder let tile: (Item) -> Tile
    @Environment(AppModel.self) private var model
    /// Where each tile sits on screen, kept off the render path (scrolling writes it).
    @State private var frames = TileFrames()

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// Order while a drag is in flight (empty = follow `items`).
    @State private var order: [String] = []
    @State private var dragging: String?
    @State private var dragX: CGFloat = 0
    @State private var startIndex = 0
    @State private var pressed: String?
    @State private var pendingRemove: Item?
    @State private var confirming = false
    /// Anchored on the first tile so late-loading tiles don't nudge the strip sideways.
    @State private var leadingId: String?

    private var step: CGFloat { tileWidth + spacing }
    private var shown: [Item] {
        guard !order.isEmpty else { return items }
        let byId = Dictionary(items.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return order.compactMap { byId[$0] } + items.filter { !order.contains($0.id) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if editing { editBar.transition(.move(edge: .top).combined(with: .opacity)) }
            strip
        }
        .animation(.spring(response: 0.38, dampingFraction: 0.82), value: editing)
        .confirmationDialog(pendingRemove.map(removeTitle) ?? "", isPresented: $confirming, titleVisibility: .visible, presenting: pendingRemove) { item in
            Button(role: .destructive) {
                Haptics.remove()
                withAnimation(.spring(response: 0.42, dampingFraction: 0.8)) { remove(item) }
                if items.count <= 1 { editing = false }
            } label: { Text(removeAction) }
            .accessibilityIdentifier("\(idPrefix)-remove-confirm")
            Button(role: .cancel) {} label: { Text("Cancel") }
        } message: { _ in Text(removeMessage) }
        .onChange(of: items.map(\.id)) { _, ids in if ids.isEmpty { editing = false } }
    }

    private var editBar: some View {
        HStack {
            Text("Hold and drag to move")
                .font(.footnote)
                .foregroundStyle(Palette.textSecondary)
            Spacer()
            Button {
                Haptics.drop()
                editing = false
            } label: {
                Text("Done editing").font(.subheadline.weight(.semibold))
                    .padding(.horizontal, 16).frame(height: 34)
                    .glassEffect(.regular.tint(Palette.action.opacity(0.9)).interactive(), in: .capsule)
                    .foregroundStyle(.white)
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("\(idPrefix)-done")
        }
        .padding(.horizontal, margin)
    }

    private var strip: some View {
        ScrollViewReader { proxy in
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: spacing) {
                    ForEach(Array(shown.enumerated()), id: \.element.id) { i, item in
                        cell(item, index: i)
                            .id(item.id)
                            .transition(.scale(scale: 0.4).combined(with: .opacity))
                    }
                }
                .scrollTargetLayout()
                .padding(.horizontal, margin)
                // Room for the minus badges that sit on the tiles' top-left corners.
                .padding(.top, editing ? 10 : 0)
                .animation(.spring(response: 0.34, dampingFraction: 0.78), value: shown.map(\.id))
            }
            .scrollPosition(id: $leadingId, anchor: .leading)
            .scrollClipDisabled()
            .scrollDisabled(dragging != nil)
            .onAppear { if leadingId == nil { leadingId = items.first?.id } }
            // A new card arriving at the front (live order) must not leave the strip scrolled
            // past it: outside edit mode the strip always starts at its first card.
            .onChange(of: items.first?.id) { _, first in
                if !editing, dragging == nil { leadingId = first }
            }
            .task(id: items.count) {
                guard scrollToEnd, items.count > 2 else { return }
                try? await Task.sleep(for: .milliseconds(700))
                withAnimation(.snappy) { proxy.scrollTo(items.last?.id, anchor: .trailing) }
            }
            .onChange(of: order) { _, o in
                // Keep the lifted tile in view as it travels.
                if let d = dragging, o.firstIndex(of: d) != nil { withAnimation(.snappy) { proxy.scrollTo(d) } }
            }
        }
    }

    @ViewBuilder
    private func cell(_ item: Item, index i: Int) -> some View {
        let lifted = dragging == item.id
        let name = title(item)
        tile(item)
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { [frames] in frames.rects[item.id] = $0 }
            .scaleEffect(lifted ? 1.07 : (pressed == item.id && !editing ? 0.96 : 1))
            .shadow(color: .black.opacity(lifted ? 0.22 : 0), radius: lifted ? 18 : 0, y: lifted ? 10 : 0)
            .modifier(Jiggle(on: editing && !lifted && !reduceMotion, seed: Self.seed(item.id)))
            .overlay(alignment: .topLeading) {
                if editing && !lifted {
                    Button {
                        Haptics.warning()
                        pendingRemove = item
                        confirming = true
                    } label: {
                        Image(systemName: "minus")
                            .font(.system(size: 12, weight: .heavy))
                            .foregroundStyle(Palette.textPrimary)
                            .frame(width: 26, height: 26)
                            .background(.regularMaterial, in: .circle)
                            .overlay(Circle().strokeBorder(.black.opacity(0.08), lineWidth: 0.5))
                            .shadow(color: .black.opacity(0.18), radius: 3, y: 1)
                            .contentShape(.circle.inset(by: -8))
                    }
                    .buttonStyle(.plain)
                    .offset(x: -9, y: -9)
                    .transition(.scale(scale: 0.2).combined(with: .opacity))
                    .accessibilityLabel(Text("Remove \(name)"))
                    .accessibilityIdentifier("\(idPrefix)-remove-\(name)")
                }
            }
            .offset(x: lifted ? dragX : 0)
            .zIndex(lifted ? 10 : 0)
            .animation(.spring(response: 0.3, dampingFraction: 0.75), value: lifted)
            .contentShape(.rect(cornerRadius: 24))
            // The tile is "on" the flipping card while its mini-app is open.
            .opacity(model.appFlip?.sourceKey == "\(idPrefix)-\(item.id)" ? 0 : 1)
            .onTapGesture {
                if editing { return }
                tapOpen(item)
            }
            .onLongPressGesture(minimumDuration: 0.45) {
                guard !editing else { return }
                Haptics.open()
                editing = true
            } onPressingChanged: { p in
                withAnimation(.spring(duration: 0.25, bounce: 0.3)) { pressed = p ? item.id : nil }
            }
            .simultaneousGesture(reorder(item), isEnabled: editing)
            .accessibilityElement(children: .contain)
            .accessibilityAddTraits(.isButton)
            .accessibilityLabel(Text(name))
            .accessibilityIdentifier("\(idPrefix)-\(name)")
            .accessibilityAction { if !editing { tapOpen(item) } }
            .accessibilityAction(named: Text("Edit")) { editing = true }
            .accessibilityAction(named: Text("Move left")) { nudge(item, by: -1) }
            .accessibilityAction(named: Text("Move right")) { nudge(item, by: 1) }
    }

    private func tapOpen(_ item: Item) {
        if let openFrom, let frame = frames.rects[item.id] {
            openFrom(item, frame, AnyView(tile(item)))
        } else {
            open(item)
        }
    }

    /// Hold a beat, then drag: a quick swipe still scrolls the strip.
    private func reorder(_ item: Item) -> some Gesture {
        LongPressGesture(minimumDuration: 0.12, maximumDistance: 12)
            .sequenced(before: DragGesture(minimumDistance: 0, coordinateSpace: .global))
            .onChanged { value in
                guard case .second(true, let drag) = value else { return }
                if dragging == nil {
                    order = items.map(\.id)
                    startIndex = order.firstIndex(of: item.id) ?? 0
                    dragging = item.id
                    Haptics.pickUp()
                }
                guard let d = drag, let i = order.firstIndex(of: item.id) else { return }
                // Where the finger is, relative to the slot the tile now occupies.
                let travelled = d.translation.width
                var x = travelled - CGFloat(i - startIndex) * step
                if x > step / 2, i < order.count - 1 {
                    order.swapAt(i, i + 1); x -= step; Haptics.selectionTick()
                } else if x < -step / 2, i > 0 {
                    order.swapAt(i, i - 1); x += step; Haptics.selectionTick()
                }
                dragX = x
            }
            .onEnded { _ in
                guard dragging != nil else { return }
                let final = order
                Haptics.drop()
                withAnimation(.spring(response: 0.36, dampingFraction: 0.78)) {
                    dragX = 0
                    dragging = nil
                    if final != items.map(\.id) { move(final) }
                    order = []
                }
            }
    }

    /// A stable small number per tile (String.hashValue changes every launch).
    static func seed(_ id: String) -> Int {
        id.unicodeScalars.reduce(7) { ($0 &* 31 &+ Int($1.value)) & 0xFFFF }
    }

    private func nudge(_ item: Item, by d: Int) {
        var ids = items.map(\.id)
        guard let i = ids.firstIndex(of: item.id) else { return }
        let j = min(max(i + d, 0), ids.count - 1)
        guard i != j else { return }
        ids.swapAt(i, j)
        withAnimation(.spring(response: 0.36, dampingFraction: 0.78)) { move(ids) }
    }
}

/// The home-screen wobble: a small rotation and bob, each tile on its own phase and speed so
/// the strip never moves in lockstep.
struct Jiggle: ViewModifier {
    var on: Bool
    var seed: Int

    func body(content: Content) -> some View {
        TimelineView(.animation(paused: !on)) { tl in
            let t = tl.date.timeIntervalSinceReferenceDate
            let f = 3.9 + Double(seed % 5) * 0.12          // ~4 Hz, a touch different per tile
            let ph = Double(seed % 97) * 0.61
            let k: Double = on ? 1 : 0
            content
                .rotationEffect(.degrees(sin(t * f * 2 * .pi + ph) * 1.6 * k))
                .offset(y: cos(t * f * 2 * .pi + ph * 1.3) * 0.7 * k)
        }
    }
}


/// Plain reference box: updating it doesn't re-render the strip.
final class TileFrames {
    var rects: [String: CGRect] = [:]
}
