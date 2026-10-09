import SwiftUI
import RodaCore

#if os(iOS)
/// The chat's top bar (v2, Muse-style): back, a horizontal title whose larger avatar overlaps
/// the left end of the glass name capsule (live status as the subtitle), and one calls capsule
/// on the right. Each piece is its own Liquid Glass (tinted like the bottom bar in dark mode)
/// with its content on top. Deployment target is iOS 26, so there's no material fallback.
struct ChatTopBar: View {
    let space: SpaceSummary
    let subtitle: String
    /// Live status that replaces the subtitle while something is happening.
    var status: ChatStatus? = nil
    let onBack: () -> Void
    let onOpen: () -> Void
    /// The title opens the header menu (the avatar keeps `onOpen`).
    var onTitle: (() -> Void)? = nil
    var menuOpen = false
    var onCall: (ZoenGlyph) -> Void = { _ in }
    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var dropped = true
    @State private var barMidY: CGFloat = 0
    static let height: CGFloat = 44
    static let callsWidth: CGFloat = CallsCapsule.width
    static let gap: CGFloat = 8
    static let avatar: CGFloat = 42
    /// How far the avatar sits over the capsule's left end.
    static let overlap: CGFloat = 14

    var body: some View {
        // A centring layout, not an HStack with spacers: the back button (44) and the calls capsule
        // (~88) differ in width, so the title is centred on the screen's midline and capped so
        // it never reaches either side (the name truncates first). All three pieces are real
        // Liquid Glass in one container; each piece's text and icons are the glass's content.
        GlassEffectContainer(spacing: 4) {
            TopBarLayout(gap: Self.gap) {
                Button(action: onBack) {
                    ZoenIcon(.back, size: 21)
                        .foregroundStyle(Palette.textPrimary)
                        .frame(width: Self.height, height: Self.height)
                        .contentShape(.circle)
                }
                .buttonStyle(GlassPress(shape: .circle))
                .accessibilityLabel(Text("Back"))
                .accessibilityIdentifier("zoenBack")

                Button(action: onTitle ?? onOpen) { title }
                    .buttonStyle(TitlePress())
                    .accessibilityHint(onTitle == nil ? Text("Shows who's in this chat") : Text("Opens the chat menu"))
                    .accessibilityIdentifier("chat-title")
                    // B centres the capsule itself under the island (the avatar hangs off
                    // its left); A centres the whole avatar + capsule group.
                    .offset(x: TitleVariant.current == .b ? -(Self.avatar - Self.overlap) / 2 : 0)
                    .modifier(IslandDrop(dropped: dropped, fromY: islandDY))

                CallsCapsule(onCall: onCall)
            }
        }
        .frame(height: Self.height)
        .onGeometryChange(for: CGFloat.self) { $0.frame(in: .global).midY } action: { barMidY = $0 }
        .onAppear {
            guard TitleVariant.current == .c, !reduceMotion, Island.rect != nil else { return }
            dropped = false
            withAnimation(.spring(duration: 0.55, bounce: 0.28).delay(0.05)) { dropped = true }
        }
        // The avatar is drawn above the container (never inside a glass layer, so it isn't
        // refracted or clipped), at the slot the title reserved for it.
        .overlayPreferenceValue(AvatarSlot.self) { slot in
            GeometryReader { g in
                if let slot {
                    let r = g[slot]
                    Button(action: onOpen) {
                        TitleAvatar(space: space, size: Self.avatar, working: status?.kind == .processing || status?.kind == .building)
                            .shadow(color: .black.opacity(0.16), radius: 2.5, y: 1)
                    }
                    .buttonStyle(TitlePress())
                    .accessibilityHidden(true)
                    .position(x: r.midX, y: r.midY)
                    .modifier(IslandDrop(dropped: dropped, fromY: islandDY))
                }
            }
        }
        .padding(.horizontal, 12)
        .overlay {
            if UserDefaults.standard.bool(forKey: "RodaCenterLine") {
                Rectangle().fill(.red).frame(width: 1).frame(height: Self.height + 16).allowsHitTesting(false)
            }
        }
    }

    /// From the bar's centre up to the island's centre (for the drop-in).
    private var islandDY: CGFloat { Island.rect.map { $0.midY - barMidY } ?? 0 }

    private var title: some View {
        // Avatar above the capsule (zIndex), overlapping its left end; the capsule's leading
        // padding keeps the text clear of it. The glass is the capsule's own effect, with the
        // text as its content, so the text renders crisp on top of the glass.
        HStack(spacing: -Self.overlap) {
            Color.clear
                .frame(width: Self.avatar, height: Self.avatar)
                .anchorPreference(key: AvatarSlot.self, value: .bounds) { $0 }
            CapWidth(max: .infinity) {
                VStack(alignment: .leading, spacing: 0) {
                    Text(space.title)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(Palette.textPrimary)
                        .lineLimit(1)
                        .minimumScaleFactor(0.65)
                        .truncationMode(.tail)
                    ZStack(alignment: .leading) {
                        if let status {
                            HStack(spacing: 3) {
                                ZoenIcon(status.glyph, size: 11)
                                Text(status.text).truncationMode(.tail)
                            }
                            .font(.caption2.weight(.medium))
                            .foregroundStyle(Palette.action)
                            .id(status.id)
                            .transition(.push(from: .bottom))
                        } else {
                            Text(subtitle)
                                .font(.caption2)
                                .foregroundStyle(Palette.textSecondary)
                                .transition(.push(from: .bottom))
                        }
                    }
                    // Room for descenders (g, p, y) inside the clip the push transition needs.
                    .padding(.bottom, 2)
                    .clipped()
                    .padding(.bottom, -2)
                    .animation(.spring(duration: 0.4, bounce: 0.15), value: status?.id)
                }
                .lineLimit(1)
            }
            .overlay(alignment: .trailing) {
                if onTitle != nil {
                    Image(systemName: "chevron.down")
                        .font(.system(size: 10, weight: .bold))
                        .foregroundStyle(Palette.textSecondary)
                        .rotationEffect(.degrees(menuOpen ? 180 : 0))
                        .animation(.spring(response: 0.35, dampingFraction: 0.7), value: menuOpen)
                        .offset(x: 14)
                        .accessibilityHidden(true)
                }
            }
            // The chevron lives in this gap, clear of the title and the capsule's end.
            .padding(.trailing, onTitle != nil ? 12 : 0)
            .frame(minWidth: TitleVariant.current == .b ? max(0, (Island.rect?.width ?? 0) - Self.overlap - 8 - 16) : 0, alignment: .leading)
            .padding(.leading, Self.overlap + 8)
            .padding(.trailing, 16)
            .frame(height: Self.height)
            .topGlass(Capsule())
        }
        .contentShape(.capsule)
    }
}

/// Top-bar glass: real `.regular.interactive()` Liquid Glass with the bottom bar's dark-mode
/// hairline, so the back button, title and calls capsule match. Applied straight to the content's own
/// shape (no container, no union), so the content always renders on top of the glass.
struct TopGlass<S: InsettableShape>: ViewModifier {
    let shape: S
    var interactive = true
    @Environment(\.colorScheme) private var scheme
    func body(content: Content) -> some View {
        content
            .glassEffect(interactive ? .regular.interactive() : .regular, in: shape)
            .barEdge(shape, diffuse: false)
    }
}

extension View {
    func topGlass<S: InsettableShape>(_ shape: S, interactive: Bool = true) -> some View { modifier(TopGlass(shape: shape, interactive: interactive)) }
}

/// Title layout variants for the Dynamic Island comparison (`-RodaTitleVariant A|B|C`).
/// A: the avatar + capsule group's optical centre on the island's centre line (default).
/// B: A, but the capsule snaps to the island's width when the name fits (a "drop").
/// C: A, plus the title drops out of the island when the chat opens.
enum TitleVariant: String {
    case a, b, c
    static var current: TitleVariant { TitleVariant(rawValue: (UserDefaults.standard.string(forKey: "RodaTitleVariant") ?? "a").lowercased()) ?? .a }
}

/// The Dynamic Island's rect in screen points, or nil on notch / home-button models (then the
/// title just uses the screen centre, which is where the island sits anyway). Island models
/// have a top safe area of 59pt or more; the island is a ~126 × 37pt pill centred
/// horizontally, ~11pt from the top (14pt on the 402/440pt-wide models).
@MainActor
enum Island {
    static var rect: CGRect? {
        guard let w = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).first,
              let win = w.windows.first(where: { $0.isKeyWindow }) ?? w.windows.first else { return nil }
        let top = win.safeAreaInsets.top, width = win.bounds.width
        guard top >= 59 else { return nil }
        let y: CGFloat = top >= 62 ? 14 : 11
        return CGRect(x: width / 2 - 63, y: y, width: 126, height: 37)
    }
}

/// C: scale and slide from the island down into place (Reduce Motion: none).
private struct IslandDrop: ViewModifier {
    let dropped: Bool
    let fromY: CGFloat
    func body(content: Content) -> some View {
        content
            .scaleEffect(dropped ? 1 : 0.42, anchor: .center)
            .offset(y: dropped ? 0 : fromY)
            .opacity(dropped ? 1 : 0.2)
    }
}

/// The bar's layout: back pinned leading, calls pinned trailing, and the title centred on the
/// bar's (= the screen's) midline, offered at most `width − 2 × max(left, right) − 2 × gap`
/// so it never reaches either side; the name truncates first. A Layout rather than a
/// measured-width state, so the cap holds on the very first pass.
private struct TopBarLayout: Layout {
    let gap: CGFloat
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let h = subviews.map { $0.sizeThatFits(.unspecified).height }.max() ?? 0
        return CGSize(width: proposal.width ?? 0, height: h)
    }
    func placeSubviews(in b: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard subviews.count == 3 else { return }
        let l = subviews[0].sizeThatFits(.unspecified), r = subviews[2].sizeThatFits(.unspecified)
        subviews[0].place(at: CGPoint(x: b.minX, y: b.midY), anchor: .leading, proposal: ProposedViewSize(l))
        subviews[2].place(at: CGPoint(x: b.maxX, y: b.midY), anchor: .trailing, proposal: ProposedViewSize(r))
        // Leave real room for long group names; sides keep their natural widths.
        let cap = max(120, b.width - l.width - r.width - 2 * gap)
        let t = subviews[1].sizeThatFits(ProposedViewSize(width: cap, height: b.height))
        subviews[1].place(at: CGPoint(x: b.midX, y: b.midY), anchor: .center, proposal: ProposedViewSize(width: min(t.width, cap), height: t.height))
    }
}

/// Proposes at most `max` width to its content and reports the content's own (natural) size,
/// so the capsule hugs short names and long ones truncate at the cap. (A `.frame(maxWidth:)`
/// would instead grow to the cap whenever more room is offered.)
private struct CapWidth: Layout {
    let max: CGFloat
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        guard let v = subviews.first else { return .zero }
        let w = proposal.width.map { min($0, max) } ?? max
        let s = v.sizeThatFits(ProposedViewSize(width: w.isFinite ? w : nil, height: proposal.height))
        return CGSize(width: min(s.width, w.isFinite ? w : s.width), height: s.height)
    }
    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        subviews.first?.place(at: bounds.origin, proposal: ProposedViewSize(width: bounds.width, height: bounds.height))
    }
}

private struct AvatarSlot: PreferenceKey {
    static let defaultValue: Anchor<CGRect>? = nil
    static func reduce(value: inout Anchor<CGRect>?, nextValue: () -> Anchor<CGRect>?) { value = value ?? nextValue() }
}

/// The title presses as one piece (avatar and capsule sink together).
struct TitlePress: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.96 : 1)
            .animation(.spring(response: 0.28, dampingFraction: 0.62), value: configuration.isPressed)
    }
}

/// Phone and video in one capsule. Calls ship with Jam: until the `jamCalls` flag is on,
/// a tap explains that instead of pretending to ring.
struct CallsCapsule: View {
    static let width: CGFloat = 2 * 42 + 4
    var onCall: (ZoenGlyph) -> Void
    @Environment(AppModel.self) private var model
    var body: some View {
        HStack(spacing: 0) {
            button(.phone, label: "Voice call")
            button(.video, label: "Video call")
        }
        .padding(.horizontal, 2)
        .frame(height: ChatTopBar.height)
        .topGlass(Capsule())
        .task {
            // `-RodaCallTap YES`: taps the phone after a beat (screenshot of the Jam toast).
            guard UserDefaults.standard.bool(forKey: "RodaCallTap") else { return }
            try? await Task.sleep(for: .seconds(3))
            model.show(ToastModel(kind: .info, text: String(localized: "Calls are coming with Jam")), seconds: 6)
        }
    }

    private func button(_ g: ZoenGlyph, label: LocalizedStringKey) -> some View {
        Button {
            if Flags.jamCalls { onCall(g) } else {
                model.show(ToastModel(kind: .info, text: String(localized: "Calls are coming with Jam")), seconds: 2.5)
            }
        } label: {
            ZoenIcon(g, size: 21)
                .foregroundStyle(Palette.textPrimary)
                .frame(width: 42, height: ChatTopBar.height)
                .contentShape(.rect)
        }
        .buttonStyle(IconPressStyle())
        .accessibilityLabel(Text(label))
    }
}

/// Glass that answers the finger: `.interactive()` gives the system shimmer and bounce,
/// and the whole capsule (glass included) sinks a touch while pressed.
struct GlassPress<S: InsettableShape>: ButtonStyle {
    let shape: S
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .environment(\.zoenIconBoil, configuration.isPressed)
            .topGlass(shape)
            .scaleEffect(configuration.isPressed ? 0.95 : 1)
            .brightness(configuration.isPressed ? -0.03 : 0)
            .animation(.spring(response: 0.28, dampingFraction: 0.62), value: configuration.isPressed)
    }
}


/// Plain Back: pops one screen. Never opens the system navigation-history menu.
///
/// - `.toolbar`: sits in a `ToolbarItem` — iOS 26 already glasses it. No second `.glassEffect`.
/// - `.floating`: our own Liquid Glass circle (chat top bar, overlays). Exactly one surface.
struct ZoenBackButton: View {
    enum Chrome { case toolbar, floating }

    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    var chrome: Chrome = .toolbar
    var size: CGFloat = 21

    var body: some View {
        Button {
            Haptics.tap()
            if !model.pop() { dismiss() }
        } label: {
            ZoenIcon(.back, size: size)
                .foregroundStyle(Palette.textPrimary)
                .frame(width: chrome == .floating ? 44 : 34, height: chrome == .floating ? 44 : 34)
                .contentShape(.circle)
        }
        .modifier(BackChrome(chrome: chrome))
        .accessibilityLabel(Text("Back"))
        .accessibilityIdentifier("zoenBack")
    }
}

private struct BackChrome: ViewModifier {
    let chrome: ZoenBackButton.Chrome
    @ViewBuilder
    func body(content: Content) -> some View {
        switch chrome {
        case .toolbar:
            // One glass: the system toolbar button. Never add .glassEffect here.
            content.buttonStyle(.plain)
        case .floating:
            content.buttonStyle(GlassPress(shape: Circle()))
        }
    }
}

#endif

/// What's happening in the chat right now, the same for people and agents (no badge: the
/// status just describes the activity). Groups name who; 1:1s don't need to.
struct ChatStatus: Equatable {
    enum Kind: String { case typing, processing, building, call }
    let kind: Kind
    var who: String? = nil
    var id: String { kind.rawValue + (who ?? "") }
    var glyph: ZoenGlyph {
        switch kind {
        case .typing: .chats
        case .processing: .sparkle
        case .building: .game
        case .call: .phone
        }
    }
    var text: String {
        if let who {
            switch kind {
            case .typing: return String(localized: "\(who) is typing…")
            case .processing: return String(localized: "\(who) is processing")
            case .building: return String(localized: "\(who) is building an app…")
            case .call: return String(localized: "\(who) is in a call")
            }
        }
        switch kind {
        case .typing: return String(localized: "is typing…")
        case .processing: return String(localized: "is processing")
        case .building: return String(localized: "is building an app…")
        case .call: return String(localized: "in a call")
        }
    }
}


#if os(iOS)
/// `-RodaTopBarWidths YES`: the v2 top bar for a few real chats at 375 and 393pt (the
/// narrowest and the common iPhone widths), over a busy striped backdrop, for the width check.
struct TopBarWidthSheet: View {
    @Environment(AppModel.self) private var model
    var body: some View {
        let byLen = model.spaces.sorted { $0.title.count > $1.title.count }
        let zoen = model.spaces.first { $0.counterpart?.handle == "zoen" }
        let rows: [(SpaceSummary, ChatStatus.Kind?)] = [byLen.first.map { ($0, .typing) }, byLen.last.map { ($0, nil) },
                                                         zoen.map { ($0, .processing) }, byLen.first.map { ($0, nil) }].compactMap { $0 }
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                ForEach([CGFloat(375), 393], id: \.self) { w in
                    Text("\(Int(w)) pt").font(.caption.monospaced()).foregroundStyle(Palette.textSecondary)
                    ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                        let sp = row.0
                        let st: ChatStatus? = row.1.map { ChatStatus(kind: $0, who: sp.counterpart == nil ? sp.members.first { !$0.isMe }?.name : nil) }
                        ChatTopBar(space: sp, subtitle: sp.counterpart != nil ? "" : String(localized: "\(sp.members.count) people"),
                                   status: st, onBack: {}, onOpen: {})
                            .padding(.vertical, 8)
                            .frame(width: w)
                            .background {
                                VStack(alignment: .leading, spacing: 2) {
                                    ForEach(0..<4) { _ in Text("Busy text behind the glass so the width check is honest").font(.footnote) }
                                }.foregroundStyle(Palette.textSecondary)
                            }
                            .overlay(Rectangle().strokeBorder(.red.opacity(0.5), lineWidth: 0.5))
                    }
                }
            }
            .padding(.top, 60)
            .frame(maxWidth: .infinity)
        }
        .background(Palette.background)
    }
}
#endif

#if os(iOS)
/// The title's face, one image only: the contact for a 1:1 (agent or person, drawn the same),
/// or the group's own picture (a generated pen-and-paper default until one is set).
struct TitleAvatar: View {
    let space: SpaceSummary
    var size: CGFloat = 42
    var working = false
    var body: some View {
        Group {
            if let c = space.counterpart {
                ContactAvatar(persona: c, size: size, working: working)
                    .profileLink(c)
                    .animation(.spring(duration: 0.45, bounce: 0.2), value: working)
            } else {
                GroupAvatar(space: space, size: size)
            }
        }
        .clipShape(.circle)
    }
}
#endif
