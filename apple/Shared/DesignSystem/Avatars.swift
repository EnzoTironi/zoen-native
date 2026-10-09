import SwiftUI
import RodaCore

enum AgentState: Equatable {
    case idle
    /// Anel animado: trabalhando.
    case working
    /// Ponto âmbar: esperando você.
    case waiting
}

/// People: real photo or monogram (never a generated face). Agents: hand-drawn art (no owner badges).
struct Avatar: View {
    let persona: Persona
    var size: CGFloat = 44
    var state: AgentState = .idle
    var budgetFraction: Double? = nil

    var body: some View {
        switch persona.kind {
        case .agent: AgentAvatar(persona: persona, size: size, state: state, budgetFraction: budgetFraction)
        case .person: PersonAvatar(persona: persona, size: size)
        }
    }
}

struct PersonAvatar: View {
    let persona: Persona
    var size: CGFloat = 44
    @State private var photo: PlatformImage?
    @State private var tick = 0

    var body: some View {
        Group {
            if let photo {
                #if canImport(UIKit)
                Image(uiImage: photo).resizable().scaledToFill()
                #else
                Image(nsImage: photo).resizable().scaledToFill()
                #endif
            } else {
                monogram
            }
        }
        .frame(width: size, height: size)
        .clipShape(.circle)
        .overlay(Circle().strokeBorder(.black.opacity(0.06), lineWidth: 0.5))
        .accessibilityLabel(persona.name)
        .task(id: "\(persona.id)-\(tick)") { photo = AvatarPhotoStore.load(persona.id) }
        .onReceive(NotificationCenter.default.publisher(for: .avatarPhotoDidChange)) { note in
            if note.object as? String == persona.id { tick &+= 1 }
        }
    }

    /// Initials on a tint derived from the persona id — never a generated face.
    private var monogram: some View {
        let tint = Color(hex: persona.tintHex)
        return Circle()
            .fill(LinearGradient(colors: [tint.mix(with: .white, by: 0.18), tint.mix(with: .black, by: 0.22)],
                                 startPoint: .topLeading, endPoint: .bottomTrailing))
            .overlay(
                Text(persona.initials)
                    .font(.system(size: size * 0.4, weight: .semibold, design: .rounded))
                    .foregroundStyle(.white)
            )
    }
}

/// A representação única de agente (§3 do documento).
struct AgentAvatar: View {
    @Environment(\.ambientPaused) private var ambientPaused
    let persona: Persona
    var size: CGFloat = 44
    var state: AgentState = .idle
    var budgetFraction: Double? = nil
    var showsOwner: Bool = false

    private var radius: CGFloat { size * 0.3 }
    private var inkSeed: UInt64 { persona.id.unicodeScalars.reduce(UInt64(5)) { $0 &* 31 &+ UInt64($1.value) } }

    @AppStorage("RodaAvatarArtVersion") private var artVersion = 0

    var body: some View {
        Group { face }.id(artVersion)
    }

    @ViewBuilder private var face: some View {
        // Animação dirigida por TimelineView (sem repeatForever em @State): some
        // limpo quando o estado muda, sem transições presas.
        if state == .working {
            TimelineView(.animation(paused: ambientPaused)) { ctx in
                let t = ctx.date.timeIntervalSinceReferenceDate
                content(angle: .degrees((t.truncatingRemainder(dividingBy: 1.6)) / 1.6 * 360),
                        scale: 0.97 + 0.03 * cos(t * 2 * .pi / 1.8))
            }
        } else {
            content(angle: nil, scale: 1)
        }
    }

    private var isZoen: Bool { persona.handle == "zoen" }

    private func content(angle: Angle?, scale: CGFloat) -> some View {
        let tint = isZoen ? Palette.action : Color(hex: persona.tintHex)
        let doodle = AgentAvatar.doodle(for: persona)
        return ZStack(alignment: .bottomTrailing) {
            // Same language as Store InkAgentAvatar: paper wash + ink doodle, never a glossy app tile.
            ZStack {
                // Paper disc under art so transparent stills never read as an empty hole.
                PaperBackground(
                    seed: inkSeed,
                    wash: isZoen
                        ? Color(hex: "#E2E8CF")
                        : (HandDrawnAvatarAsset.pickAgent(handle: persona.handle, id: persona.id)
                            .map { Color(hex: $0.backdrop) } ?? tint.opacity(0.55))
                )
                if isZoen {
                    MascotHead(size: size * 1.05, working: state == .working).offset(y: size * 0.04)
                } else if let art = HandDrawnAvatarAsset.pickAgent(handle: persona.handle, id: persona.id) {
                    HandDrawnAvatarView(asset: art, size: size)
                } else {
                    DoodleView(doodle: doodle, drawOn: 0, freezeAt: 3.0)
                        .padding(size * 0.14)
                }
            }
            .frame(width: size, height: size)
            .clipShape(.circle)
            .overlay {
                InkDrawing(seed: inkSeed &+ 41, drawOn: 0, fps: 8, breathe: 0, jitter: 0.55) { _ in
                    [InkStroke.ellipse(0.5, 0.5, 0.475, 0.475,
                                       width: max(0.02, 1.6 / size),
                                       color: InkPalette.ink.opacity(0.75), n: 14)]
                }
            }
            .scaleEffect(scale)
            .overlay {
                if angle != nil {
                    InkOutline(corner: 0.5, color: tint, width: 0.08, seed: inkSeed &+ 1, loop: 1.4)
                        .padding(-max(3, size * 0.1))
                }
            }
        }
        .overlay(alignment: .topTrailing) {
            if state == .waiting {
                Circle().fill(Palette.amber)
                    .frame(width: size * 0.26, height: size * 0.26)
                    .overlay(Circle().stroke(Palette.background, lineWidth: 2))
                    .offset(x: size * 0.08, y: -size * 0.08)
            }
        }
        .frame(width: size, height: size)
        .accessibilityElement()
        .accessibilityLabel("\(persona.name), agent\(persona.ownerName.map { String(localized: " of \($0)") } ?? "")")
    }
}


extension AgentAvatar {
    static func doodle(for persona: Persona) -> Doodle {
        switch persona.handle {
        case "financeiro", "finance": .notepad
        case "organizador", "organizer": .ballot
        case "guia", "guide": .trip
        case "moss": .moss
        default:
            // Fall back by glyph so demo seed agents still get ink, not a blank wash.
            switch persona.glyph {
            case "list.clipboard", "doc.text", "creditcard": .notepad
            case "briefcase", "checklist", "calendar": .tasks
            case "map", "figure.walk": .trip
            default: .roda
            }
        }
    }
}

/// Anyone in a chat, drawn the same way: a round face. Agents are contacts — no tile,
/// owner badge or kind badge. Art: people = photo or monogram; agents = hand-drawn (mascot/doodle).
struct ContactAvatar: View {
    let persona: Persona
    var size: CGFloat = 26
    /// Zoen switches to its working pose (same size) while it's busy.
    var working = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    @AppStorage("RodaAvatarArtVersion") private var artVersion = 0

    var body: some View {
        if persona.kind == .agent {
            agentFace.id(artVersion)
        } else {
            PersonAvatar(persona: persona, size: size)
        }
    }

    private var agentFace: some View {
        let tint = Color(hex: persona.tintHex)
        let zoen = persona.handle == "zoen"
        let seed = persona.id.unicodeScalars.reduce(UInt64(9)) { $0 &* 31 &+ UInt64($1.value) }
        return ZStack {
            PaperBackground(seed: seed, wash: zoen ? Color(hex: "#E2E8CF") : tint.opacity(0.45))
            if zoen {
                MascotHead(size: size * 1.08, working: working && !reduceMotion).offset(y: size * 0.05)
                    .id(working)
                    .transition(.opacity.combined(with: .scale(scale: 0.9)))
            } else if let art = HandDrawnAvatarAsset.pickAgent(handle: persona.handle, id: persona.id) {
                HandDrawnAvatarView(asset: art, size: size)
            } else {
                DoodleView(doodle: agentDoodle,
                           drawOn: (reduceMotion || size < 40) ? 0 : 1.1,
                           freezeAt: (reduceMotion || size < 48) ? 2.0 : nil)
                    .padding(size * 0.12)
            }
        }
        .frame(width: size, height: size)
        .clipShape(.circle)
        .overlay {
            InkDrawing(seed: seed &+ 41, drawOn: 0, fps: 8, breathe: 0, jitter: 0.5) { _ in
                [InkStroke.ellipse(0.5, 0.5, 0.475, 0.475, width: max(0.02, 1.6 / size), color: InkPalette.ink.opacity(0.7), n: 14)]
            }
        }
        .accessibilityLabel(persona.name)
    }

    private var agentDoodle: Doodle { AgentAvatar.doodle(for: persona) }
}

/// Avatar de um Espaço: a outra ponta (DM), pilha (grupo) ou capa (comunidade).
struct SpaceAvatar: View {
    let space: SpaceSummary
    var size: CGFloat = 52
    var waiting: Bool = false
    var working: Bool = false

    var body: some View {
        Group {
            if let c = space.counterpart {
                Avatar(persona: c, size: size, state: working ? .working : (waiting ? .waiting : .idle))
            } else if space.kind == .community || space.kind == .group {
                GroupAvatar(space: space, size: size)
            } else {
                let people = space.members.filter { !$0.isMe }
                let first = people.first(where: { $0.kind == .person }) ?? people.first
                let second = people.first(where: { $0.kind == .agent && $0.id != first?.id })
                ZStack(alignment: .bottomTrailing) {
                    if let first { Avatar(persona: first, size: size * 0.78).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading) }
                    if let second {
                        AgentAvatar(persona: second, size: size * 0.56, state: working ? .working : .idle, showsOwner: false)
                            .padding(2)
                            .background(Palette.background, in: .rect(cornerRadius: size * 0.2, style: .continuous))
                    }
                }
                .frame(width: size, height: size)
                .overlay(alignment: .topTrailing) {
                    if waiting {
                        Circle().fill(Palette.amber).frame(width: size * 0.24, height: size * 0.24)
                            .overlay(Circle().stroke(Palette.background, lineWidth: 2))
                    }
                }
            }
        }
    }
}

/// Fila de rostos (cabeçalho do Espaço).
struct FacePile: View {
    let members: [Persona]
    var size: CGFloat = 22
    var body: some View {
        HStack(spacing: -size * 0.32) {
            ForEach(members.prefix(5)) { m in
                Avatar(persona: m, size: size)
                    .overlay {
                        if m.kind == .person { Circle().stroke(Palette.background, lineWidth: 1.5) }
                    }
                    .profileLink(m, enabled: !m.isMe)
                    .zIndex(m.isMe ? 0 : 1)
            }
        }
    }
}

/// A group's picture: its own photo once one is set; until then a default in our pen-and-paper
/// style, a doodle that fits the group on a coloured circle. Stable per group (keyed by its name, so it survives a demo reset). The choice is
/// stored under `RodaGroupAvatar.<id>` (doodle name) so chat info can change it later.
struct GroupAvatar: View {
    let space: SpaceSummary
    var size: CGFloat = 42

    static let fills: [Color] = [InkPalette.butter, InkPalette.mint, InkPalette.sky, InkPalette.lilac, InkPalette.blush, InkPalette.cardboard]

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @AppStorage("RodaAvatarArtVersion") private var artVersion = 0

    var body: some View {
        let h = Self.hash(space.title)
        // Prefer bundled hand-drawn still when the asset set lands; else animated doodle
        // with a freeze for lists / Reduce Motion.
        Circle()
            .fill(LinearGradient(colors: [fill.mix(with: .white, by: 0.12), fill.mix(with: .black, by: 0.06)], startPoint: .top, endPoint: .bottom))
            .overlay {
                if let art = HandDrawnAvatarAsset.pickGroup(spaceId: space.id, title: space.title) {
                    HandDrawnAvatarView(asset: art, size: size)
                } else {
                    DoodleView(doodle: doodle, drawOn: reduceMotion ? 0 : 1.1,
                               freezeAt: (reduceMotion || size < 48) ? 2.0 : nil)
                        .padding(size * 0.1)
                }
            }
            .clipShape(.circle)
            .frame(width: size, height: size)
            .accessibilityLabel(space.title)
            .id("\(h)-\(artVersion)")
    }

    private var fill: Color { Self.fills[Self.hash(space.title + "fill") % Self.fills.count] }

    private var doodle: Doodle {
        if let saved = UserDefaults.standard.string(forKey: "RodaGroupAvatar.\(space.id)").flatMap(Doodle.named) { return saved }
        let t = space.title.lowercased()
        let rules: [([String], Doodle)] = [
            (["hike", "trail", "trilha", "crew", "saturday", "sábado", "turma"], .hike),
            (["viajantes", "litoral", "coastal", "travelers", "praia", "beach", "trip", "travel", "viagem", "paraty"], .trip),
            (["dinner", "jantar", "food", "cook", "pot"], .pot), (["music", "vinyl", "música", "band"], .vinyl),
            (["garden", "plant", "jardim", "moss"], .moss), (["vote", "poll", "votação"], .ballot),
        ]
        for (keys, d) in rules where keys.contains(where: t.contains) { return d }
        let pool: [Doodle] = [.heart, .pin, .trophy, .notepad, .steps, .hourglass]
        return pool[Self.hash(space.title) % pool.count]
    }

    static func hash(_ s: String) -> Int { Int(s.unicodeScalars.reduce(UInt32(2166136261)) { ($0 ^ $1.value) &* 16777619 } % 9973) }
}
