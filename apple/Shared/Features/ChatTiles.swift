import SwiftUI
import RodaCore

// MARK: - Pinned tiles under the chat's top bar

/// Live mini-apps of a chat as Wabi-style widget tiles, drawn by the same `SnapshotCard`
/// renderer as Home (and WidgetKit). Two fit per row on iPhone; more scroll sideways.
/// Tap opens the mini-app; long press enters edit mode for reordering and unpinning.
/// The strip stays above the chat's scrolling history.
struct ChatPinStrip: View {
    @Environment(AppModel.self) private var model
    let apps: [ItemDetail]
    var onOpenItem: (String) -> Void
    var compact = false
    @State private var avail: CGFloat = 402
    /// Jiggle edit mode (long-press a tile): drag to reorder, minus to unpin.
    @State private var editing = false

    private var side: CGFloat { min(170, max(140, (avail - 34) / 2)) }

    /// One tile of a mini-app ("<item>#<n>": a hike shows several).
    struct Tile: Identifiable { let id: String; let item: ItemDetail; let snap: WidgetSnapshot }

    var body: some View {
        let raw = apps.flatMap { item in ChatTiles.tiles(for: item).enumerated().map { Tile(id: "\(item.id)#\($0.offset)", item: item, snap: $0.element) } }
        let order = model.chatTileOrder
        let tiles = raw.enumerated().sorted { a, b in
            let ia = order.firstIndex(of: a.element.id) ?? (order.count + a.offset)
            let ib = order.firstIndex(of: b.element.id) ?? (order.count + b.offset)
            return ia < ib
        }.map(\.element)
        if !tiles.isEmpty {
            Group {
                if compact { chips(tiles) } else { editable(tiles) }
            }
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { avail = $0 }
            .padding(.bottom, compact ? 4 : 6)
            .animation(.spring(duration: 0.4, bounce: 0.15), value: compact)
            .onChange(of: compact) { _, c in if c { editing = false } }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("chat-pinned-apps")
        }
    }

    private func editable(_ tiles: [Tile]) -> some View {
        EditableTileStrip(
            items: tiles, tileWidth: side, spacing: 10, margin: 12, idPrefix: "pin-tile",
            editing: $editing,
            title: { $0.snap.title },
            open: { t in Haptics.open(); open(t.item) },
            openFrom: { t, frame, front in
                if t.item.plan != nil { onOpenItem(t.item.id) }
                else { model.flipOpenApp(t.item.id, from: frame, sourceKey: "pin-tile-\(t.id)", front: front) }
            },
            move: { ids in model.setChatTileOrder(ids) },
            remove: { t in model.unpinChatApp(t.item.id) },
            removeTitle: { t in String(localized: "Unpin “\(t.snap.title)” from the top of the chat?") },
            removeMessage: "The mini-app stays in the chat as a card.",
            removeAction: "Unpin",
            scrollToEnd: UserDefaults.standard.bool(forKey: "RodaPinScroll")
        ) { t in
            SnapshotCard(snap: t.snap, side: side, live: true, corner: 24, height: (side / 1.06).rounded())
        }
        // A horizontal ScrollView is greedy vertically; the row is exactly as tall as a tile.
        .fixedSize(horizontal: false, vertical: true)
    }

    private func chips(_ tiles: [Tile]) -> some View {
        ScrollView(.horizontal) {
            HStack(spacing: 8) {
                ForEach(tiles) { t in
                    Button { Haptics.open(); open(t.item) } label: { chip(t.snap) }
                        .buttonStyle(PressScaleStyle())
                        .menuPreviewShape(.rect(cornerRadius: 18, style: .continuous))
                        .contextMenu { menu(t.item, t.snap) }
                        .accessibilityHint(Text("Opens the mini-app"))
                }
            }
            .padding(.horizontal, 12)
        }
        .scrollIndicators(.hidden)
        .scrollClipDisabled()
        .fixedSize(horizontal: false, vertical: true)
    }

    /// Compact form: a glass chip with a thumbnail and the tile's title.
    private func chip(_ s: WidgetSnapshot) -> some View {
        HStack(spacing: 7) {
            Group {
                if let photo = s.photo { Image(photo).resizable().scaledToFill() }
                else { Image(systemName: s.symbol).font(.system(size: 13, weight: .semibold)).foregroundStyle(Color(hex: s.accentHex)) }
            }
            .frame(width: 26, height: 26)
            .clipShape(.rect(cornerRadius: 8, style: .continuous))
            Text(s.title).font(.footnote.weight(.semibold)).foregroundStyle(Palette.textPrimary).lineLimit(1)
        }
        .padding(.leading, 5).padding(.trailing, 12).frame(height: 36)
        .glassEffect(.regular.interactive(), in: .capsule)
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder private func menu(_ item: ItemDetail, _ s: WidgetSnapshot) -> some View {
        Button { open(item) } label: { Label { Text("Open") } icon: { ZoenGlyph.chevron.menuImage } }
        ShareLink(item: item.plan == nil ? "\(s.title) · \(s.deepLink)" : "\(s.title) · \(s.detail ?? "")") {
            Label { Text("Share") } icon: { ZoenGlyph.share.menuImage }
        }
        Button { withAnimation(.snappy) { model.unpinChatApp(item.id) } } label: { Label { Text("Unpin") } icon: { ZoenGlyph.pin.menuImage } }
    }

    private func open(_ item: ItemDetail) {
        if item.plan != nil { onOpenItem(item.id) }
        else { model.openApp(item.id) }
    }
}

// MARK: - Tiles a mini-app shows in its chat

@MainActor
enum ChatTiles {
    static func tiles(for item: ItemDetail) -> [WidgetSnapshot] {
        if let plan = item.plan {
            let count = plan.sections.flatMap(\.lines).count
            var snap = WidgetSnapshot(id: item.id, appId: "plan", template: .caption,
                                      title: item.title, accentHex: "#3D7A28", symbol: "list.bullet",
                                      art: "notepad", deepLink: "zoen://app/\(item.id)")
            snap.detail = String(localized: "\(count) items · \(Money.format(plan.totalCents))")
            return snap.validated().map { [$0] } ?? []
        }
        guard let app = item.app else { return [] }
        if app.appId == "hike" { return hike(item, app) }
        return WidgetSnapshot.from(item).map { [$0] } ?? []
    }

    /// Hike: the trail's photo (name + day), then a vote tile until it's locked in and a
    /// glass countdown after; once the day is planned, the ride as a ticket.
    static func hike(_ item: ItemDetail, _ app: AppStateDto) -> [WidgetSnapshot] {
        let h = HikeInfo(app)
        let trail = h.decided ?? h.leader ?? "tomales"
        let link = "zoen://app/\(item.id)"
        func snap(_ template: WidgetSnapshot.Template, _ title: String) -> WidgetSnapshot {
            WidgetSnapshot(id: item.id, appId: "hike", template: template, title: title, accentHex: "#3D7A28", symbol: "figure.hiking", deepLink: link)
        }
        var out: [WidgetSnapshot] = []
        var photo = snap(.photo, h.displayName(trail))
        photo.eyebrow = h.day.isEmpty ? nil : h.day
        photo.photo = "hike-\(trail)"
        out.append(photo)
        if h.decided != nil {
            var c = snap(.photo, h.title)
            c.photo = "hike-\(trail)"
            c.targetMs = nextSaturdayMorning()
            out.append(c)
        } else {
            var v = snap(.stat, String(localized: "Vote: 3 trails"))
            v.value = "\(h.total)"
            v.detail = h.total == 0 ? String(localized: "No votes yet") : String(localized: "\(h.displayName(h.leader ?? "tomales")) leads")
            v.art = "ballot"
            out.append(v)
        }
        let steps = AppView(app).array("itinerary")
        if let first = steps.first?["time"] as? String, let last = steps.last?["time"] as? String, steps.count > 1 {
            var t = snap(.ticket, String(localized: "Ride"))
            t.eyebrow = saturdayLabel()
            t.codes = AppLocale.isPortuguese
                ? ["SP", ["tomales": "PDS", "steep": "PGR", "lands": "JRG"][trail] ?? "TRL"]
                : ["SF", ["tomales": "TML", "steep": "STR", "lands": "LDE"][trail] ?? "TRL"]
            t.places = [AppLocale.isPortuguese ? "São Paulo" : "San Francisco", h.displayName(trail)]
            t.times = [first, last]
            t.symbol = "car.fill"
            out.append(t)
        }
        return out.compactMap { $0.validated() }
    }

    static func nextSaturdayMorning(after now: Date = .now) -> Int64 {
        var c = DateComponents(); c.weekday = 7; c.hour = 8; c.minute = 0
        let d = Calendar.current.nextDate(after: now, matching: c, matchingPolicy: .nextTime) ?? now
        return Int64(d.timeIntervalSince1970 * 1000)
    }

    static func saturdayLabel() -> String {
        let d = Date(timeIntervalSince1970: Double(nextSaturdayMorning()) / 1000)
        return d.formatted(.dateTime.weekday(.abbreviated).month(.abbreviated).day())
    }
}
