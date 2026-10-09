import SwiftUI
import RodaCore

// MARK: - Full screen (the React mini-app, edge to edge)

/// The hike runs as a bundled MCP App (React + MapLibre) in the sandboxed web view. It draws
/// its own chrome (header pill, Compare, carousel, detail and compare pages); the close
/// button inside it asks the host to go back inline, which dismisses this sheet.
struct HikeSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.miniAppClose) private var miniAppClose
    let item: ItemDetail
    @State private var height: CGFloat = 800

    var body: some View {
        McpAppWebView(itemId: item.id, displayMode: "fullscreen", height: $height, onClose: { (miniAppClose ?? { dismiss() })() }, edgeToEdge: true)
            .ignoresSafeArea()
            .background(Color(hex: "#ECE8DA").ignoresSafeArea())
            .presentationDetents([.large])
            .presentationDragIndicator(.hidden)
            // Pans belong to the map; the app's own close button dismisses.
            .interactiveDismissDisabled()
    }
}

// MARK: - Shared reading of the hike state

struct HikeInfo {
    let title: String
    let day: String
    let decided: String?
    let votes: [(trail: String, names: [String])]
    private let trailLabels: [String: String]
    let firstStep: String?

    init(_ app: AppStateDto) {
        let v = AppView(app)
        title = v.string("title") ?? app.name
        day = v.string("day") ?? ""
        decided = v.string("decided")
        votes = v.array("trails").map { (($0["id"] as? String) ?? "", ($0["votes"] as? [String]) ?? []) }
        trailLabels = Dictionary(uniqueKeysWithValues: v.array("trails").compactMap { row -> (String, String)? in
            guard let id = row["id"] as? String else { return nil }
            if let n = row["name"] as? String, !n.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return (id, n) }
            return (id, Self.name(id))
        })
        firstStep = v.array("itinerary").first.map { "\(($0["time"] as? String) ?? "") \(($0["text"] as? String) ?? "")" }
    }

    var total: Int { votes.reduce(0) { $0 + $1.names.count } }
    var voters: [String] { votes.flatMap(\.names) }
    var leader: String? { votes.max { $0.names.count < $1.names.count }.flatMap { $0.names.isEmpty ? nil : $0.trail } }
    var photo: String { "hike-" + (decided ?? leader ?? "tomales") }

    func displayName(_ id: String) -> String { trailLabels[id] ?? Self.name(id) }

    static func name(_ id: String) -> String {
        // Locale-aware labels (asset ids stay tomales/steep/lands).
        if AppLocale.isPortuguese {
            return ["tomales": "Praia do Sono", "steep": "Pedra Grande", "lands": "Pico do Jaraguá"][id] ?? id
        }
        return ["tomales": "Tomales Point", "steep": "Steep Ravine", "lands": "Lands End"][id] ?? id
    }

    var status: String {
        if let decided { return "\(displayName(decided)) • \(day)" }
        if total == 0 { return String(localized: "3 trails • Maps & photos") }
        return String(localized: "\(total) votes • \(displayName(leader ?? "tomales")) leads")
    }
}

extension AppModel {
    /// A person in any of your chats, by display name (votes are stored by name).
    func persona(named name: String) -> Persona? {
        if me?.name == name { return me }
        return spaces.lazy.flatMap(\.members).first { $0.name == name }
    }
}

struct VoterStack: View {
    @Environment(AppModel.self) private var model
    let names: [String]
    var size: CGFloat = 22
    var limit = 4
    var body: some View {
        HStack(spacing: -size * 0.34) {
            ForEach(Array(Set(names)).sorted().prefix(limit), id: \.self) { n in
                if let p = model.persona(named: n) {
                    // The ring is the card colour, so dark mode doesn't get bright halos.
                    Avatar(persona: p, size: size).overlay(Circle().strokeBorder(Palette.surfaceRaised, lineWidth: 1.5))
                }
            }
        }
    }
}

// MARK: - Chat card (agent output)

/// What Zoen posts: a clean card with a photo, the title and "3 trails • Maps & photos".
/// It turns into the vote count, then the choice, live for everyone.
struct HikeChatCard: View {
    @Environment(AppModel.self) private var model
    @Environment(\.appZoom) private var zoom
    let card: ItemCard
    let app: AppStateDto

    var body: some View {
        let h = HikeInfo(app)
        Button { Haptics.open(); model.openApp(card.itemId) } label: {
            HStack(spacing: 12) {
                Image(h.photo)
                    .resizable().scaledToFill()
                    .frame(width: 58, height: 58)
                    .clipShape(.rect(cornerRadius: 14, style: .continuous))
                    .contentTransition(.opacity)
                // Title and status win the width: two lines before an ellipsis, faces shrink to 3.
                VStack(alignment: .leading, spacing: 3) {
                    Text(h.title).font(.body.weight(.semibold)).foregroundStyle(Palette.textPrimary)
                        .lineLimit(2).minimumScaleFactor(0.9).fixedSize(horizontal: false, vertical: true)
                    Text(h.status).font(.subheadline).foregroundStyle(Palette.textSecondary)
                        .lineLimit(1).minimumScaleFactor(0.85)
                        .contentTransition(.numericText())
                }
                .layoutPriority(1)
                Spacer(minLength: 6)
                if !h.voters.isEmpty { VoterStack(names: h.voters, size: 20, limit: 3) }
                ZoenIcon(.chevron, size: 14).foregroundStyle(Palette.textTertiary)
            }
            .padding(10)
            .frame(minWidth: 270, maxWidth: 300, alignment: .leading)
            .background(Palette.surfaceRaised, in: .rect(cornerRadius: 22, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 22, style: .continuous).strokeBorder(Palette.textPrimary.opacity(0.06), lineWidth: 0.5))
            .shadow(color: .black.opacity(0.07), radius: 12, y: 5)
        }
        .buttonStyle(PressScaleStyle())
        .appZoomSource(card.itemId, zoom)
        .animation(.spring(duration: 0.5, bounce: 0.25), value: h.total)
        .accessibilityLabel("\(h.title), \(h.status)")
        .accessibilityHint("Opens the map, trails and voting")
        .contextMenu {
            let onHome = model.isOnHome(card.itemId)
            Button {
                onHome ? model.unpinFromHome(card.itemId) : model.pinToHome(card.itemId)
            } label: { Label { Text(onHome ? "Unpin from Home" : "Pin to Home") } icon: { ZoenGlyph.pin.menuImage } }
        }
    }
}

// MARK: - Itinerary (agent text with timed lines → a native timeline)

struct Itinerary {
    let header: String
    let steps: [(time: String, text: String)]

    /// At least three "8:15 Something" lines make an itinerary; anything else stays text.
    init?(_ text: String) {
        var head: [String] = []
        var steps: [(String, String)] = []
        for raw in text.split(separator: "\n") {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if let r = line.range(of: #"^\d{1,2}:\d{2}\s"#, options: .regularExpression) {
                steps.append((String(line[r]).trimmingCharacters(in: .whitespaces), String(line[r.upperBound...])))
            } else if steps.isEmpty {
                head.append(line)
            }
        }
        guard steps.count >= 3 else { return nil }
        header = head.joined(separator: " ")
        self.steps = steps
    }
}

struct ItineraryCard: View {
    let itinerary: Itinerary
    @State private var shown = 0
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if !itinerary.header.isEmpty {
                Text(itinerary.header).font(.body).foregroundStyle(Palette.textPrimary)
            }
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(itinerary.steps.enumerated()), id: \.offset) { i, s in
                    HStack(alignment: .top, spacing: 12) {
                        Text(s.time)
                            .font(.subheadline.weight(.bold).monospacedDigit())
                            .foregroundStyle(Palette.textPrimary)
                            .frame(width: 44, alignment: .trailing)
                        VStack(spacing: 0) {
                            Circle().fill(i == 0 ? Color(hex: "#3D7A28") : Color(hex: "#3D7A28").opacity(0.35))
                                .frame(width: 10, height: 10)
                                .padding(.top, 5)
                            if i < itinerary.steps.count - 1 {
                                Rectangle().fill(Color(hex: "#3D7A28").opacity(0.25)).frame(width: 2).frame(maxHeight: .infinity)
                            }
                        }
                        .frame(width: 10)
                        Text(s.text).font(.subheadline).foregroundStyle(Palette.textPrimary)
                            .fixedSize(horizontal: false, vertical: true)
                            .padding(.bottom, 14)
                        Spacer(minLength: 0)
                    }
                    .opacity(i < shown ? 1 : 0)
                    .offset(y: i < shown ? 0 : 8)
                }
            }
            .padding(.horizontal, 12)
            .padding(.top, 14)
            .padding(.bottom, 2)
            .frame(maxWidth: 300, alignment: .leading)
            .background(Palette.surfaceRaised, in: .rect(cornerRadius: 22, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 22, style: .continuous).strokeBorder(Palette.textPrimary.opacity(0.06), lineWidth: 0.5))
            .shadow(color: .black.opacity(0.06), radius: 10, y: 4)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(([itinerary.header] + itinerary.steps.map { "\($0.time), \($0.text)" }).joined(separator: ". "))
        .task {
            if reduceMotion { shown = itinerary.steps.count; return }
            for i in 1...itinerary.steps.count {
                withAnimation(.spring(duration: 0.45, bounce: 0.2)) { shown = i }
                try? await Task.sleep(for: .milliseconds(90))
            }
        }
    }
}

// MARK: - Settings: what mini-apps may use

/// Every device grant a mini-app holds, from the core's Grant table. Revoking is a signed
/// GrantRevoked event; the next request asks again.
struct MiniAppAccessSection: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let grants = model.core.appDeviceGrants()
        Section {
            if grants.isEmpty {
                Text("No mini-app has asked for anything yet.").font(.subheadline).foregroundStyle(Palette.textSecondary)
            }
            ForEach(grants, id: \.grantId) { g in
                HStack(spacing: 12) {
                    Image(systemName: CapabilityCopy.symbol(g.capability)).foregroundStyle(Palette.action)
                        .frame(width: 32, height: 32)
                        .background(Palette.action.opacity(0.12), in: .rect(cornerRadius: 9, style: .continuous))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(CapabilityCopy.data(g.capability)).font(.subheadline.weight(.semibold))
                        Text("\(g.appName) · \(g.spaceTitle) · \(g.always ? String(localized: "always") : String(localized: "once"))")
                            .font(.caption).foregroundStyle(Palette.textSecondary)
                    }
                    Spacer()
                    Button("Revoke", role: .destructive) {
                        Haptics.commit()
                        model.perform { try model.core.revokeAppDevice(grantId: g.grantId) }
                    }
                    .buttonStyle(.borderless)
                    .font(.subheadline.weight(.semibold))
                }
            }
        } header: {
            Text("Mini-app access")
        } footer: {
            Text("Each permission is for one mini-app in one chat. iOS keeps its own switch in Settings too.")
        }
    }
}
