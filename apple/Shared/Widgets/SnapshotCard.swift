import SwiftUI

/// Draws a `WidgetSnapshot`: the one renderer behind the Home strip and the widgets.
///
/// Content only (no app name, no "Open" label): the art says which mini-app it is and the
/// numbers say what changed. `live` animates the hand-drawn art in the app; widgets pass
/// `false` and get a still frame.
struct SnapshotCard: View {
    enum Family { case small, medium }

    let snap: WidgetSnapshot
    var family: Family = .small
    var side: CGFloat = 170
    var live = true
    var corner: CGFloat = 28
    /// Taller or shorter than square (chat tiles are a touch wider than tall).
    var height: CGFloat? = nil

    private var accent: Color { Color(hex: snap.accentHex) }
    private var onDark: Bool { snap.template == .photo || ["globe", "trip", "pet.asleep"].contains(snap.art ?? "") }
    private var ink: Color { onDark ? .white : InkPalette.ink }
    private var width: CGFloat { family == .small ? side : side * 2 + 14 }
    private var shape: RoundedRectangle { RoundedRectangle(cornerRadius: corner, style: .continuous) }

    var body: some View {
        ZStack(alignment: .topLeading) {
            background
            content
                .padding(14)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
        .frame(width: width, height: height ?? side)
        // Full bleed: the art/photo IS the tile, clipped by the tile's own continuous corner.
        // No card, padding or fill behind it (only text templates like the ticket paint
        // their own paper as content). A 0.5pt hairline and a soft shadow separate it.
        .clipShape(shape)
        .overlay(shape.strokeBorder(Palette.textPrimary.opacity(0.1), lineWidth: 0.5))
        .contentShape(shape)
        .menuPreviewShape(shape)
        .contentShape(.dragPreview, shape)
        .shadow(color: .black.opacity(live ? 0.1 : 0), radius: 8, y: 3)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilityText)
    }

    // MARK: Background art

    @ViewBuilder private var background: some View {
        if snap.template == .photo, let photo = snap.photo {
            // In an overlay so a fill-scaled photo can't widen the tile's layout.
            Color.clear
                .overlay {
                    Image(photo).resizable().scaledToFill()
                        // The countdown tile shows a closer crop so it doesn't repeat its neighbour.
                        .scaleEffect(snap.targetMs != nil ? 1.5 : 1, anchor: .bottomLeading)
                }
                .clipped()
                .overlay(LinearGradient(colors: [.clear, .black.opacity(snap.targetMs != nil ? 0.1 : 0.45)], startPoint: .center, endPoint: .bottom))
        } else if snap.template == .ticket {
            LinearGradient(colors: [Color.adaptive(light: "#FCFCFD", dark: "#2B2F38"), Color.adaptive(light: "#E6E7EB", dark: "#1D2027")], startPoint: .top, endPoint: .bottom)
        } else {
            artBackground
        }
    }

    @ViewBuilder private var artBackground: some View {
        switch snap.art {
        case "pet", "pet.asleep", "pet.gone":
            PetBackdrop(asleep: snap.art == "pet.asleep")
        case "trip":
            LinearGradient(colors: [Color(hex: "#F9B67A"), Color(hex: "#E9846B"), Color(hex: "#3B6C8F")], startPoint: .top, endPoint: .bottom)
        case "hike":
            LinearGradient(colors: [Color(hex: "#DDEFD2"), Color(hex: "#B9DDB0")], startPoint: .top, endPoint: .bottom)
        case "globe":
            Color.black
        case "pot":
            LinearGradient(colors: [Color(hex: "#FBE3C8"), Color(hex: "#F4B98A")], startPoint: .top, endPoint: .bottom)
        default:
            LinearGradient(colors: [accent.opacity(0.22), InkPalette.paper], startPoint: .top, endPoint: .bottom)
        }
    }

    @ViewBuilder private func art(_ size: CGFloat) -> some View {
        let freeze: Double? = live ? nil : 4
        switch snap.art {
        case "pet", "pet.asleep", "pet.gone":
            PetSprite(asleep: snap.art == "pet.asleep", faded: snap.art == "pet.gone", bounce: live).frame(width: size, height: size)
        case "trip": DoodleView(doodle: .trip, freezeAt: freeze).frame(width: size, height: size)
        case "hike": DoodleView(doodle: .hike, freezeAt: freeze).frame(width: size, height: size)
        case "globe": GlobeView(spin: live, interactive: false).frame(width: size, height: size)
        case "pot": DoodleView(doodle: .pot, freezeAt: freeze).frame(width: size, height: size)
        case "ballot": DoodleView(doodle: .ballot, freezeAt: freeze).frame(width: size, height: size)
        case "notepad": DoodleView(doodle: .notepad, freezeAt: freeze).frame(width: size, height: size)
        default: Image(systemName: snap.symbol).font(.system(size: size * 0.4, weight: .semibold)).foregroundStyle(accent)
        }
    }

    // MARK: Templates

    @ViewBuilder private var content: some View {
        switch snap.template {
        case .progress: progress
        case .countdown: countdown
        case .list: list
        case .stat: stat
        case .caption: caption
        case .ticket: ticket
        case .photo: photoTile
        }
    }

    // MARK: Ticket (route card: date, big codes, route line, times)

    private var ticket: some View {
        let codes = snap.codes ?? [], places = snap.places ?? [], times = snap.times ?? []
        let tInk = Palette.textPrimary
        return VStack(alignment: .leading, spacing: 0) {
            Text(snap.eyebrow ?? snap.title).font(.system(size: 10, weight: .semibold)).foregroundStyle(tInk.opacity(0.6))
            Spacer(minLength: 4)
            HStack(alignment: .firstTextBaseline) {
                Text(codes.first ?? "")
                Spacer(minLength: 4)
                Text(codes.last ?? "")
            }
            .font(.system(size: side * 0.16, weight: .semibold, design: .rounded))
            .foregroundStyle(tInk)
            HStack {
                Text(places.first ?? "")
                Spacer(minLength: 4)
                Text(places.count > 1 ? places[1] : "")
            }
            .font(.system(size: 9, weight: .medium))
            .foregroundStyle(tInk.opacity(0.55))
            .lineLimit(1)
            Spacer(minLength: 4)
            HStack(spacing: 6) {
                Capsule().fill(tInk.opacity(0.4)).frame(height: 1.2)
                Image(systemName: snap.symbol).font(.system(size: 12, weight: .semibold)).foregroundStyle(tInk.opacity(0.8))
                DashLine().stroke(tInk.opacity(0.35), style: StrokeStyle(lineWidth: 1.2, dash: [3, 3])).frame(height: 1.2)
            }
            Spacer(minLength: 4)
            Rectangle().fill(tInk.opacity(0.08)).frame(height: 0.7).padding(.bottom, 6)
            HStack(alignment: .bottom) {
                VStack(alignment: .leading, spacing: 1) {
                    Text("Departure").font(.system(size: 9, weight: .medium)).foregroundStyle(tInk.opacity(0.55))
                    Text(times.first ?? "").font(.system(size: 14, weight: .semibold, design: .rounded)).monospacedDigit()
                }
                Spacer(minLength: 4)
                VStack(alignment: .trailing, spacing: 1) {
                    Text("Arrival").font(.system(size: 9, weight: .medium)).foregroundStyle(tInk.opacity(0.55))
                    Text(times.count > 1 ? times[1] : "").font(.system(size: 14, weight: .semibold, design: .rounded)).monospacedDigit()
                }
            }
            .foregroundStyle(tInk)
        }
    }

    // MARK: Photo (image + name, or image + glass countdown)

    private var photoTile: some View {
        TimelineView(.periodic(from: .now, by: 60)) { ctx in
            let r = snap.remaining(at: ctx.date)
            if snap.targetMs != nil {
                VStack(spacing: 7) {
                    Text(snap.title)
                        .font(.system(size: side * 0.12, weight: .bold, design: .rounded))
                        .foregroundStyle(.white)
                        .lineLimit(1).minimumScaleFactor(0.6)
                        .shadow(color: .black.opacity(0.25), radius: 3, y: 1)
                    HStack(spacing: 5) {
                        cell(r.days, Text("DAYS"))
                        cell(r.hours, Text("HRS"))
                        cell(r.minutes, Text("MIN"))
                    }
                }
                .padding(.horizontal, 9)
                .padding(.vertical, 9)
                .modifier(GlassPanel(live: live, corner: 16))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .animation(.spring(duration: 0.5), value: r.minutes)
            } else {
                VStack(alignment: .leading, spacing: 1) {
                    Spacer(minLength: 0)
                    Text(snap.title).font(.system(.headline, design: .rounded).weight(.bold)).lineLimit(2)
                    if let e = snap.eyebrow { Text(e).font(.caption.weight(.semibold)).opacity(0.9) }
                }
                .foregroundStyle(.white)
                .shadow(color: .black.opacity(0.35), radius: 4, y: 1)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomLeading)
            }
        }
    }

    private func cell(_ n: Int, _ label: Text) -> some View {
        VStack(spacing: 3) {
            Text(String(format: "%02d", min(n, 99)))
                .font(.system(size: side * 0.13, weight: .heavy, design: .rounded))
                .monospacedDigit()
                .foregroundStyle(.white)
                .contentTransition(.numericText(countsDown: true))
                .frame(width: side * 0.2, height: side * 0.18)
                .background(.black.opacity(0.78), in: .rect(cornerRadius: 7, style: .continuous))
            label.font(.system(size: 8, weight: .bold)).foregroundStyle(.white.opacity(0.92))
        }
    }

    private var title: some View {
        Text(snap.title)
            .font(.system(.subheadline, design: .rounded).weight(.bold))
            .foregroundStyle(ink)
            .lineLimit(2)
            .contentTransition(.numericText())
    }

    private var progress: some View {
        HStack(alignment: .bottom, spacing: 14) {
            VStack(alignment: .leading, spacing: 0) {
                title
                Spacer(minLength: 0)
                if snap.art?.hasPrefix("pet") == true {
                    art(side * 0.5).frame(maxWidth: .infinity).offset(y: 4)
                    Spacer(minLength: 0)
                }
                bars(compact: family == .small)
            }
            if family == .medium {
                VStack(alignment: .leading, spacing: 6) {
                    if let d = snap.detail { Text(d).font(.footnote.weight(.semibold)).foregroundStyle(ink.opacity(0.75)).lineLimit(2) }
                    Spacer(minLength: 0)
                    actionPills
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    /// Three slim gauges. On the small card they sit side by side without labels (the
    /// labels go to VoiceOver); on medium each gets its word.
    private func bars(compact: Bool) -> some View {
        let bars = snap.bars ?? []
        return Group {
            if compact && snap.art?.hasPrefix("pet") == true {
                HStack(spacing: 6) {
                    ForEach(bars, id: \.label) { b in gauge(b.value).frame(height: 6) }
                }
            } else {
                VStack(alignment: .leading, spacing: 5) {
                    ForEach(bars, id: \.label) { b in
                        HStack(spacing: 6) {
                            Text(b.label).font(.caption2.weight(.semibold)).foregroundStyle(ink.opacity(0.75)).lineLimit(1)
                                .frame(width: compact ? 52 : 64, alignment: .leading)
                            gauge(b.value).frame(height: 6)
                        }
                    }
                }
            }
        }
    }

    private func gauge(_ v: Double) -> some View {
        GeometryReader { g in
            ZStack(alignment: .leading) {
                Capsule().fill(ink.opacity(0.14))
                Capsule().fill(v < 0.25 ? Color(hex: "#E5484D") : accent)
                    .frame(width: max(6, g.size.width * v))
            }
        }
        .animation(.spring(duration: 0.6, bounce: 0.25), value: v)
    }

    private var countdown: some View {
        TimelineView(.periodic(from: .now, by: 60)) { ctx in
            let r = snap.remaining(at: ctx.date)
            ZStack(alignment: .topLeading) {
                art(side * 0.82)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                    .offset(x: 14, y: 12)
                    .opacity(0.95)
                VStack(alignment: .leading, spacing: 0) {
                    title
                    Spacer(minLength: 0)
                    if r.days > 0 {
                        Text("\(r.days)")
                            .font(.system(size: side * 0.32, weight: .heavy, design: .rounded))
                            .monospacedDigit()
                            .contentTransition(.numericText(countsDown: true))
                        Text(r.days == 1 ? "day to go" : "days to go")
                            .font(.caption.weight(.bold))
                            .opacity(0.85)
                    } else {
                        Text("\(r.hours)h \(r.minutes)m")
                            .font(.system(size: side * 0.2, weight: .heavy, design: .rounded))
                            .monospacedDigit()
                            .contentTransition(.numericText(countsDown: true))
                    }
                }
                .foregroundStyle(.white)
                .shadow(color: .black.opacity(0.18), radius: 6, y: 2)
            }
            .animation(.spring(duration: 0.5), value: r.days)
        }
    }

    private var list: some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack(alignment: .firstTextBaseline) {
                title.lineLimit(1)
                Spacer(minLength: 4)
                if let v = snap.value {
                    Text(v).font(.caption.weight(.heavy)).monospacedDigit().foregroundStyle(accent)
                        .contentTransition(.numericText())
                }
            }
            ForEach(Array((snap.rows ?? []).prefix(family == .small ? 4 : 4).enumerated()), id: \.offset) { _, row in
                HStack(spacing: 7) {
                    Image(systemName: row.done ? "checkmark.circle.fill" : "circle")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(row.done ? accent : ink.opacity(0.35))
                    Text(row.text)
                        .font(.footnote.weight(.medium))
                        .foregroundStyle(ink.opacity(row.done ? 0.45 : 0.9))
                        .strikethrough(row.done, color: ink.opacity(0.35))
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 0)
        }
    }

    private var stat: some View {
        ZStack(alignment: .topLeading) {
            art(side * 0.62)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: snap.art == "globe" ? .center : .bottomTrailing)
                .offset(x: snap.art == "globe" ? 0 : 10, y: snap.art == "globe" ? 10 : 8)
            VStack(alignment: .leading, spacing: 2) {
                title.lineLimit(1)
                Spacer(minLength: 0)
                if let v = snap.value {
                    Text(v).font(.system(size: side * 0.2, weight: .heavy, design: .rounded)).monospacedDigit()
                        .foregroundStyle(ink).contentTransition(.numericText())
                }
                if let d = snap.detail { Text(d).font(.caption2.weight(.semibold)).foregroundStyle(ink.opacity(0.7)).lineLimit(1) }
            }
        }
    }

    private var caption: some View {
        VStack(alignment: .leading, spacing: 6) {
            title
            if let d = snap.detail { Text(d).font(.footnote).foregroundStyle(ink.opacity(0.75)).lineLimit(3) }
            Spacer(minLength: 0)
            art(side * 0.4).frame(maxWidth: .infinity, alignment: .trailing)
        }
    }

    @ViewBuilder private var actionPills: some View {
        if let actions = snap.actions, !actions.isEmpty {
            HStack(spacing: 6) {
                ForEach(actions, id: \.tool) { a in
                    Text(a.label).font(.caption.weight(.bold)).foregroundStyle(.white)
                        .padding(.horizontal, 12).padding(.vertical, 7)
                        .background(accent, in: .capsule)
                }
            }
        }
    }

    private var accessibilityText: String {
        var parts = [snap.title]
        if let e = snap.eyebrow { parts.append(e) }
        switch snap.template {
        case .countdown:
            let r = snap.remaining()
            parts.append(String(localized: "\(r.days) days to go"))
        case .photo where snap.targetMs != nil:
            let r = snap.remaining()
            parts.append(String(localized: "\(r.days) days, \(r.hours) hours and \(r.minutes) minutes to go"))
        case .ticket:
            let pl = snap.places ?? [], t = snap.times ?? []
            if pl.count == 2, t.count == 2 { parts.append(String(localized: "from \(pl[0]) at \(t[0]) to \(pl[1]) at \(t[1])")) }
        default:
            if let v = snap.value { parts.append(v) }
            parts += (snap.bars ?? []).map { "\($0.label) \(Int($0.value * 100))%" }
            parts += (snap.rows ?? []).map { $0.done ? String(localized: "\($0.text), done") : $0.text }
        }
        if let d = snap.detail { parts.append(d) }
        return parts.joined(separator: ", ")
    }
}

/// The ticket's dashed half of the route line.
struct DashLine: Shape {
    func path(in r: CGRect) -> Path {
        var p = Path()
        p.move(to: CGPoint(x: r.minX, y: r.midY))
        p.addLine(to: CGPoint(x: r.maxX, y: r.midY))
        return p
    }
}

/// Liquid Glass in the app; a plain dark wash in WidgetKit (which draws no glass).
struct GlassPanel: ViewModifier {
    var live: Bool
    var corner: CGFloat
    func body(content: Content) -> some View {
        if live {
            // Clear glass with a light dark tint: the photo shows through instead of a milky
            // white block.
            content.glassEffect(.clear.tint(.black.opacity(0.14)), in: .rect(cornerRadius: corner, style: .continuous))
        } else {
            content.background(.black.opacity(0.28), in: .rect(cornerRadius: corner, style: .continuous))
        }
    }
}

/// Home strip placeholder: Zo with an empty frame, when nothing is pinned.
struct PinHereCard: View {
    var side: CGFloat = 170
    var body: some View {
        VStack(spacing: 6) {
            MascotView(pose: .wave).frame(width: side * 0.56, height: side * 0.56)
            Text("Pin a mini-app here")
                .font(.footnote.weight(.semibold))
                .foregroundStyle(Palette.textSecondary)
                .multilineTextAlignment(.center)
        }
        .padding(12)
        .frame(width: side, height: side)
        .background(Palette.surfaceMuted.opacity(0.5), in: .rect(cornerRadius: 28, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 28, style: .continuous).strokeBorder(Palette.textTertiary.opacity(0.5), style: StrokeStyle(lineWidth: 1.4, dash: [5, 5])))
        .accessibilityElement(children: .combine)
    }
}
