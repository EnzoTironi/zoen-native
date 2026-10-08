import SwiftUI
import RodaCore

// MARK: - Sprites (pixel art)

enum DonkeyArt {
    /// Jumento de perfil (3/4), cabeção de chibi: orelhas altas com rosa por dentro,
    /// crina escura, focinho claro, barriga clara e rabo com tufo.
    static let sideRows: [String] = [
        "................OO.OO.....",
        "...............OPO.OPO....",
        "...............OPO.OPO....",
        "...............OPOOOPO....",
        "..............OMGGGGGGO...",
        ".............OMMGGGGGGGO..",
        ".............OMGGGEEGGGGO.",
        "............OMMGGGEHGGGGO.",
        "............OMGGGGGGGGWWWO",
        "...........OMMGGGGGGWWWWWO",
        "...........OMGGGGGGWWWNWWO",
        "..OOOOOOOOOMMGGGGGGOWWWWO.",
        ".OGGGGGGGGGGGGGGGGGGOOOO..",
        "OGGGGGGGGGGGGGGGGGGGO.....",
        "OGGGGGGGGGGGGGGGGGGGO.....",
        "TOGGGGGGGGGGGGGGGGGGO.....",
        "T.OGGWWWWWWWWWWWGGGO......",
        "T..OGGGGGGGGGGGGGGO.......",
    ]
    static let legsA = ["...OGO..OGO....OGO..OGO...", "...OGO..OGO....OGO..OGO...", "...OGO..OGO....OGO..OGO...", "...OKO..OKO....OKO..OKO..."]
    static let legsB = ["....OGO.OGO.....OGO.OGO...", "....OGO.OGO.....OGO.OGO...", "....OGO.OGO.....OGO.OGO...", "....OKO.OKO.....OKO.OKO..."]
    static let width = 26, height = 22
    static let front: [[Character]] = (sideRows + legsA).map(Array.init)

    /// Dormindo: olhos fechados (um traço) e deitado (sem pernas, mais baixo).
    static let sleeping: [[Character]] = {
        var rows = sideRows.map(Array.init)
        rows[6] = rows[6].map { $0 == "E" ? "G" : $0 }
        rows[7] = rows[7].map { $0 == "E" || $0 == "H" ? "O" : $0 }
        let blank = Array(repeating: Character("."), count: width)
        let belly = Array("...OOOO.........OOOO......")
        return [blank, blank, blank] + rows + [belly]
    }()

    /// Idle blink: eyes shut for a beat.
    static let blinking: [[Character]] = {
        var rows = (sideRows + legsA).map(Array.init)
        rows[6] = rows[6].map { $0 == "E" ? "G" : $0 }
        rows[7] = rows[7].map { $0 == "E" || $0 == "H" ? "O" : $0 }
        return rows
    }()

    static let sideA: [[Character]] = (sideRows + legsA).map(Array.init)
    static let sideB: [[Character]] = (sideRows + legsB).map(Array.init)

    static let palette: [Character: Color] = [
        "O": Color(hex: "#3A3340"), "G": Color(hex: "#9C97A3"), "M": Color(hex: "#4B4452"), "P": Color(hex: "#F4A3A8"),
        "W": Color(hex: "#F3EEE8"), "E": Color(hex: "#17131C"), "H": Color(hex: "#FFFFFF"), "N": Color(hex: "#6E6673"), "K": Color(hex: "#2C2730"), "T": Color(hex: "#4B4452"),
    ]

    static func draw(_ rows: [[Character]], in ctx: GraphicsContext, origin: CGPoint, px: CGFloat, opacity: Double = 1) {
        for (y, row) in rows.enumerated() {
            for (x, ch) in row.enumerated() where ch != "." {
                if let c = palette[ch] {
                    ctx.fill(Path(CGRect(x: origin.x + CGFloat(x) * px, y: origin.y + CGFloat(y) * px, width: px + 0.3, height: px + 0.3)), with: .color(c.opacity(opacity)))
                }
            }
        }
    }
}

struct PetSprite: View {
    var asleep = false
    var faded = false
    var bounce = true
    /// When the donkey last got food: it munches a pixel carrot for ~1.6 s.
    var eatingSince: Date? = nil
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 20, paused: !bounce || reduceMotion)) { tl in
            let t = tl.date.timeIntervalSinceReferenceDate
            Canvas { ctx, size in
                let animated = bounce && !reduceMotion
                let eat = eatingSince.map { tl.date.timeIntervalSince($0) } ?? 99
                let eating = !asleep && eat >= 0 && eat < 1.6
                // Idle: blink every few seconds. Eating: the head bobs on each bite.
                let blink = animated && !asleep && (t.truncatingRemainder(dividingBy: 3.9) > 3.75)
                let rows = asleep ? DonkeyArt.sleeping : (blink ? DonkeyArt.blinking : DonkeyArt.front)
                let px = floor(min(size.width / CGFloat(DonkeyArt.width + 4), size.height / CGFloat(DonkeyArt.height)) * 2) / 2
                let w = px * CGFloat(DonkeyArt.width), h = px * CGFloat(DonkeyArt.height)
                var oy = (size.height - h) / 2
                if animated && !asleep && !eating { oy -= abs(sin(t * 3.2)) * px * 1.5 }
                if asleep && animated { oy += sin(t * 1.4) * px * 0.4 }
                let bite = eating ? Int(eat / 0.4) : 0
                if eating && Int(eat / 0.2) % 2 == 1 { oy += px }
                let ox = (size.width - w) / 2 - px * 2
                DonkeyArt.draw(rows, in: ctx, origin: CGPoint(x: ox, y: oy), px: max(px, 1), opacity: faded ? 0.35 : 1)
                if eating {
                    // A pixel carrot at the muzzle, shorter after every bite, with crumbs.
                    let len = max(0, 5 - bite)
                    let cx = ox + px * 26, cy = oy + px * 11
                    for i in 0..<len {
                        ctx.fill(Path(CGRect(x: cx + CGFloat(i) * px, y: cy, width: px + 0.3, height: px + 0.3)), with: .color(Color(hex: "#F58A2C")))
                    }
                    if len > 0 {
                        ctx.fill(Path(CGRect(x: cx + CGFloat(len) * px, y: cy - px, width: px + 0.3, height: px + 0.3)), with: .color(Color(hex: "#5DB04B")))
                        ctx.fill(Path(CGRect(x: cx + CGFloat(len) * px, y: cy + px, width: px + 0.3, height: px + 0.3)), with: .color(Color(hex: "#5DB04B")))
                    }
                    if Int(eat / 0.2) % 2 == 1 {
                        ctx.fill(Path(CGRect(x: cx - px * 0.5, y: cy + px * 3, width: px * 0.7, height: px * 0.7)), with: .color(Color(hex: "#F58A2C").opacity(0.8)))
                        ctx.fill(Path(CGRect(x: cx + px * 1.5, y: cy + px * 4, width: px * 0.6, height: px * 0.6)), with: .color(Color(hex: "#F58A2C").opacity(0.6)))
                    }
                }
            }
            .overlay(alignment: .topTrailing) {
                if asleep { SleepyZs(t: t, animated: bounce && !reduceMotion) }
            }
        }
        .accessibilityHidden(true)
    }
}

struct SleepyZs: View {
    let t: Double
    var animated = true
    var body: some View {
        ZStack {
            ForEach(0..<3, id: \.self) { i in
                let phase = animated ? (t * 0.5 + Double(i) / 3).truncatingRemainder(dividingBy: 1) : Double(i) / 3
                Text("z")
                    .font(.system(size: 10 + CGFloat(i) * 4, weight: .heavy, design: .monospaced))
                    .foregroundStyle(.white.opacity(1 - phase))
                    .offset(x: CGFloat(i) * 7 + phase * 6, y: -phase * 22 - CGFloat(i) * 6)
            }
        }
        .offset(x: -6, y: 18)
    }
}

/// Fundo do Wabi: gradiente vertical claro de dia, escuro dormindo.
struct PetBackdrop: View {
    var asleep: Bool
    var body: some View {
        LinearGradient(colors: asleep ? [Color(hex: "#2C3054"), Color(hex: "#6D5A80"), Color(hex: "#C9A3A0")]
                                      : [Color(hex: "#C4D8E2"), Color(hex: "#DCDCD6"), Color(hex: "#F2DFCD")],
                       startPoint: .top, endPoint: .bottom)
            .animation(.easeInOut(duration: 0.8), value: asleep)
    }
}

// MARK: - Folha do pet

enum PetTab: String, CaseIterable { case care, play, board
    var title: String { switch self { case .care: String(localized: "Care"); case .play: String(localized: "Play"); case .board: String(localized: "Leaderboard") } }
    var symbol: String { switch self { case .care: "heart"; case .play: "gamecontroller"; case .board: "trophy" } }
}

struct PetSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let item: ItemDetail
    let app: AppStateDto
    @State private var renaming = false
    @State private var newName = ""
    @State private var emotes = false
    @State private var floating: [Emote] = []
    @State private var eatAt: Date?
    @State private var unboxing = false

    var body: some View {
        @Bindable var model = model
        let v = AppView(app)
        let asleep = v.bool("asleep")
        ZStack(alignment: .bottom) {
            PetBackdrop(asleep: asleep).ignoresSafeArea()
            Group {
                switch model.petTab {
                case .care: care(v, asleep: asleep)
                case .play: DashGameView(item: item, app: app)
                case .board: PetBoard(app: app)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .padding(.bottom, 84)

            PetTabBar(tab: $model.petTab, dark: asleep || model.petTab == .play)
                .padding(.bottom, 10)
        }
        .overlay(alignment: .topTrailing) {
            if model.petTab == .care {
                Button { withAnimation(.spring) { emotes.toggle() } } label: {
                    Label("Emotes", systemImage: "face.smiling").font(.footnote.weight(.semibold))
                        .padding(.horizontal, 12).padding(.vertical, 7)
                }
                .buttonStyle(.plain)
                .foregroundStyle(asleep ? .white : Color(hex: "#1C1C1E"))
                .glassEffect(.regular.interactive(), in: .capsule)
                .padding(.top, 18).padding(.trailing, 16)
            }
        }
        .overlay(alignment: .topLeading) {
            Button { dismiss() } label: { Image(systemName: "chevron.down").font(.body.weight(.semibold)).frame(width: 36, height: 36) }
                .buttonStyle(.plain)
                .foregroundStyle(asleep || model.petTab == .play ? .white : Color(hex: "#1C1C1E"))
                .glassEffect(.regular.interactive(), in: .circle)
                .padding(.top, 16).padding(.leading, 16)
                .accessibilityLabel("Close")
        }
        .alert("New name", isPresented: $renaming) {
            TextField("Name", text: $newName)
            Button("Cancel", role: .cancel) {}
            Button("Rename") { call("pet_rename", ["name": newName]) }
        } message: { Text("Changes it for everyone in the group.") }
        .overlay {
            if unboxing {
                UnboxingReveal(name: v.string("name") ?? item.title, freezeAt: Self.unboxFreeze) {
                    withAnimation(.easeOut(duration: 0.35)) { unboxing = false }
                }
                .transition(.opacity)
            }
        }
        .onAppear {
            // The first time someone opens a newly adopted donkey, it arrives in a box.
            let d = UserDefaults.standard
            let key = "RodaUnboxed." + item.id
            let story = d.string(forKey: "RodaStory")
            if story == "pet-unbox" || (story == nil && !d.bool(forKey: key) && !v.bool("released")) { unboxing = true }
            d.set(true, forKey: key)
        }
        .presentationDetents([.large])
        .presentationBackground(.clear)
    }

    /// `-RodaUnboxFreeze 2.6` freezes the reveal at that moment (screenshots).
    private static var unboxFreeze: Double? {
        let v = UserDefaults.standard.double(forKey: "RodaUnboxFreeze")
        return v > 0 ? v : nil
    }

    @ViewBuilder
    private func care(_ v: AppView, asleep: Bool) -> some View {
        let name = v.string("name") ?? item.title
        ScrollView {
            VStack(spacing: 18) {
                ZStack {
                    PetSprite(asleep: asleep, faded: v.bool("released"), eatingSince: eatAt)
                        .frame(width: 210, height: 210)
                        .onChange(of: v.double("fullness")) { old, new in
                            if new > old + 0.01 { eatAt = .now }
                        }
                    ForEach(floating) { f in FloatingEmote(emoji: f.emoji) }
                }
                .padding(.top, 54)
                VStack(spacing: 4) {
                    Button { newName = name; renaming = true } label: {
                        HStack(spacing: 4) {
                            Text(name).font(.title3.weight(.heavy))
                            Image(systemName: "pencil").font(.caption.weight(.bold)).opacity(0.5)
                        }
                    }
                    .buttonStyle(.plain)
                    .accessibilityHint("Rename")
                    Text(v.string("mood") ?? "")
                        .font(.system(.subheadline, design: .monospaced).weight(.bold))
                        .contentTransition(.opacity)
                }
                .foregroundStyle(asleep ? .white : Color(hex: "#1C1C1E"))

                if emotes {
                    HStack(spacing: 14) {
                        ForEach(["❤️", "🥕", "😂", "🫶", "😴"], id: \.self) { e in
                            Button { emote(e) } label: { Text(e).font(.title2) }.buttonStyle(.plain)
                        }
                    }
                    .padding(.horizontal, 16).padding(.vertical, 8)
                    .glassEffect(.regular, in: .capsule)
                    .transition(.scale.combined(with: .opacity))
                }

                VStack(spacing: 12) {
                    PetBar(label: String(localized: "Food"), value: v.double("fullness"), dark: asleep)
                    PetBar(label: String(localized: "Mood"), value: v.double("joy"), dark: asleep)
                    PetBar(label: String(localized: "Rest"), value: v.double("energy"), dark: asleep)
                }
                .padding(.horizontal, 28)

                VStack(spacing: 10) {
                    HStack(spacing: 10) {
                        Button { call("pet_feed") } label: { Label("Feed", systemImage: "carrot.fill") }
                            .buttonStyle(WabiPill(dark: asleep))
                            .disabled(asleep)
                            .opacity(asleep ? 0.45 : 1)
                        Button { call(asleep ? "pet_wake" : "pet_nap") } label: {
                            Label(asleep ? String(localized: "Wake") : String(localized: "Nap"), systemImage: asleep ? "sun.max.fill" : "moon.fill")
                                .contentTransition(.symbolEffect(.replace))
                        }
                        .buttonStyle(WabiPill(dark: asleep))
                    }
                    Button { withAnimation(.spring) { model.petTab = .play } } label: { Label("Donkey Dash", systemImage: "gamecontroller.fill") }
                        .buttonStyle(WabiPill(dark: asleep))
                }
                .padding(.horizontal, 24)

                if let last = v.log.first {
                    Text("\(last.who) \(last.what)")
                        .font(.footnote)
                        .foregroundStyle(asleep ? .white.opacity(0.8) : Color(hex: "#3A3A3C"))
                        .multilineTextAlignment(.center)
                        .padding(.horizontal, 24)
                        .contentTransition(.opacity)
                }
            }
            .frame(maxWidth: 520)
            .frame(maxWidth: .infinity)
        }
        .scrollBounceBehavior(.basedOnSize)
        .animation(.spring(duration: 0.45, bounce: 0.25), value: asleep)
    }

    private func call(_ tool: String, _ args: [String: Any] = [:]) {
        let json = (try? JSONSerialization.data(withJSONObject: args)).flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
        model.callAppTool(item.id, tool, args: json)
        Haptics.action()
        if tool == "pet_feed" { emote("🥕") }
    }

    private func emote(_ e: String) {
        Haptics.selectionTick()
        let em = Emote(emoji: e)
        floating.append(em)
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(1.5))
            floating.removeAll { $0.id == em.id }
        }
    }
}

struct Emote: Identifiable { let id = UUID(); let emoji: String }

struct BoardRow: Identifiable { var id: String { name }; let name: String; let a: Int; let b: Int }

struct FloatingEmote: View {
    let emoji: String
    @State private var go = false
    private let dx = CGFloat.random(in: -50...50)
    var body: some View {
        Text(emoji).font(.system(size: 34))
            .offset(x: go ? dx : 0, y: go ? -110 : 0)
            .opacity(go ? 0 : 1)
            .scaleEffect(go ? 1.3 : 0.6)
            .onAppear { withAnimation(.easeOut(duration: 1.4)) { go = true } }
            .allowsHitTesting(false)
    }
}

struct PetBar: View {
    let label: String
    let value: Double
    var dark = false
    var body: some View {
        HStack(spacing: 12) {
            Text(label).font(.footnote.weight(.semibold)).frame(width: 72, alignment: .leading)
            GeometryReader { g in
                ZStack(alignment: .leading) {
                    Capsule().fill(dark ? .white.opacity(0.18) : Color.black.opacity(0.10))
                    Capsule().fill(Color(hex: "#4F7CFF")).frame(width: max(6, g.size.width * value / 100))
                }
            }
            .frame(height: 6)
            Text("\(Int(value))").font(.footnote.monospacedDigit()).frame(width: 28, alignment: .trailing).contentTransition(.numericText())
        }
        .foregroundStyle(dark ? .white : Color(hex: "#3A3A3C"))
        .animation(.spring(duration: 0.6, bounce: 0.3), value: value)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(label) \(Int(value)) of 100")
    }
}

struct PetTabBar: View {
    @Binding var tab: PetTab
    var dark = false
    var body: some View {
        HStack(spacing: 4) {
            ForEach(PetTab.allCases, id: \.self) { t in
                Button { withAnimation(.spring(duration: 0.35)) { tab = t }; Haptics.selectionTick() } label: {
                    VStack(spacing: 3) {
                        Image(systemName: t == tab ? t.symbol + ".fill" : t.symbol).font(.system(size: 17, weight: .semibold))
                        Text(t.title).font(.caption2.weight(.semibold))
                    }
                    .frame(maxWidth: .infinity, minHeight: 50)
                    .background { if t == tab { Capsule().fill(dark ? .white.opacity(0.18) : .white.opacity(0.7)) } }
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(t == tab ? .isSelected : [])
            }
        }
        .foregroundStyle(dark ? .white : Color(hex: "#1C1C1E"))
        .padding(5)
        .glassEffect(.regular, in: .capsule)
        .padding(.horizontal, 22)
    }
}

// MARK: - Placar

struct PetBoard: View {
    @Environment(AppModel.self) private var model
    let app: AppStateDto
    var body: some View {
        let v = AppView(app)
        let dash = v.dict("dash")
        let best = (dash["best"] as? [String: [String: Any]]) ?? [:]
        let rows = best.map { BoardRow(name: $0.key, a: ($0.value["meters"] as? NSNumber)?.intValue ?? 0, b: ($0.value["carrots"] as? NSNumber)?.intValue ?? 0) }
            .sorted { $0.a > $1.a }
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                Text("Group leaderboard").font(.title2.weight(.heavy)).padding(.top, 64)
                Text("Donkey Dash · \((dash["runs"] as? NSNumber)?.intValue ?? 0) runs").font(.footnote).foregroundStyle(.secondary)
                if rows.isEmpty {
                    InkEmptyState(pose: .zen, title: String(localized: "Nobody has run yet. The first run sets the record."), size: 110)
                }
                ForEach(Array(rows.enumerated()), id: \.element.id) { i, r in
                    HStack(spacing: 12) {
                        Text(["🥇", "🥈", "🥉"].indices.contains(i) ? ["🥇", "🥈", "🥉"][i] : "\(i + 1)").font(.title3).frame(width: 30)
                        if let p = model.personaNamed(r.name) { Avatar(persona: p, size: 30) }
                        Text(r.name).font(.body.weight(.semibold))
                        Spacer()
                        VStack(alignment: .trailing, spacing: 1) {
                            Text("\(r.a) m").font(.system(.body, design: .monospaced).weight(.bold))
                            Text("\(r.b) 🥕").font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    .padding(12)
                    .background(.white.opacity(0.75), in: .rect(cornerRadius: 16, style: .continuous))
                }
            }
            .padding(.horizontal, 20)
            .frame(maxWidth: 520)
            .frame(maxWidth: .infinity)
        }
        .foregroundStyle(Color(hex: "#1C1C1E"))
    }
}

// MARK: - Corrida do Jumento (o "dino" do Chrome, com cenouras)

@MainActor
final class DashGame {
    enum Phase { case ready, running, paused, over }
    var phase: Phase = .ready
    var y: Double = 0          // altura do pulo (px de mundo)
    var vy: Double = 0
    var distance: Double = 0   // em "metros"
    var speed: Double = 160    // px/s
    var carrots = 0
    var obstacles: [Double] = []   // x em px de mundo (relativo à tela)
    var carrotsX: [(x: Double, h: Double)] = []
    var plusOnes: [(x: Double, y: Double, t: Double)] = []
    var toast: (text: String, until: Double)?
    var last: Double?
    var spawnIn: Double = 1.2
    var carrotIn: Double = 0.7
    var auto = false
    var frame = 0

    func reset() {
        y = 0; vy = 0; distance = 0; speed = 160; carrots = 0; obstacles = []; carrotsX = []; plusOnes = []; toast = nil; last = nil
        spawnIn = 1.2; carrotIn = 0.7; phase = .running
    }

    func jump(now: Double) {
        if phase == .ready || phase == .over { reset(); return }
        guard phase == .running, y <= 0.5 else { return }
        vy = 420
        Haptics.selectionTick()
    }

    /// Avança a simulação até `now`; devolve true se acabou a corrida neste passo.
    func step(now: Double, width: Double) -> Bool {
        defer { last = now }
        guard phase == .running, let last else { return false }
        let dt = min(1 / 20, now - last)
        frame += 1
        speed += dt * 6
        distance += speed * dt / 18
        // física do pulo
        vy -= 1300 * dt
        y = max(0, y + vy * dt)
        if y == 0 { vy = 0 }
        // mundo anda
        obstacles = obstacles.map { $0 - speed * dt }.filter { $0 > -30 }
        carrotsX = carrotsX.map { ($0.x - speed * dt, $0.h) }.filter { $0.x > -30 }
        plusOnes = plusOnes.filter { now - $0.t < 0.8 }
        spawnIn -= dt; carrotIn -= dt
        if spawnIn <= 0 { obstacles.append(width + 20); spawnIn = Double.random(in: 1.1...2.1) * 160 / speed + 0.5 }
        if carrotIn <= 0 { carrotsX.append((width + 20, Double([0, 46, 70].randomElement()!))); carrotIn = Double.random(in: 0.8...1.6) }
        if auto, let next = obstacles.first(where: { $0 > 60 }), next < 60 + speed * 0.32, y == 0 { vy = 420 }
        // colisões (jumento em x 40...72)
        let bx = 42.0, bw = 40.0
        for o in obstacles where o < bx + bw && o + 12 > bx {
            if y < 26 {
                phase = .over
                Haptics.dismiss()
                return true
            }
        }
        var kept: [(x: Double, h: Double)] = []
        for c in carrotsX {
            if c.x < bx + bw && c.x + 10 > bx && abs(y + 12 - c.h - 6) < 24 {
                carrots += 1
                plusOnes.append((c.x, c.h, now))
                Haptics.action()
                if carrots % 5 == 0 { toast = (String(localized: "you got \(carrots) carrots!"), now + 1.4) }
            } else { kept.append(c) }
        }
        carrotsX = kept
        if y > 60, toast == nil || (toast?.until ?? 0) < now, frame % 90 == 0 { toast = (String(localized: "nice jump!"), now + 1.0) }
        return false
    }
}

struct DashGameView: View {
    @Environment(AppModel.self) private var model
    let item: ItemDetail
    let app: AppStateDto
    @State private var game = DashGame()
    @State private var result: (meters: Int, carrots: Int)?
    /// O jogo não é @Observable (roda a 60 fps no Canvas); isto avisa a view das trocas de fase.
    @State private var bump = 0

    var body: some View {
        let _ = bump
        let v = AppView(app)
        let best = ((v.dict("dash")["best"] as? [String: [String: Any]])?[model.me?.name ?? ""]?["meters"] as? NSNumber)?.intValue
        GeometryReader { geo in
            TimelineView(.animation(paused: game.phase != .running)) { tl in
                let now = tl.date.timeIntervalSinceReferenceDate
                let _ = tick(now: now, width: geo.size.width)
                ZStack(alignment: .top) {
                    Canvas { ctx, size in draw(ctx, size: size, now: now) }
                    VStack(alignment: .leading, spacing: 2) {
                        Text("\(Int(game.distance)) m")
                        Text("\(game.carrots) carrots")
                        Text("best \(best.map { "\($0) m" } ?? "—")").opacity(0.7)
                    }
                    .font(.system(size: 15, weight: .bold, design: .monospaced))
                    .foregroundStyle(Color(hex: "#1C1C1E"))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.top, 70).padding(.leading, 28)
                    if let t = game.toast, t.until > now {
                        Text(t.text)
                            .font(.system(size: 14, weight: .heavy, design: .monospaced))
                            .padding(.horizontal, 10).padding(.vertical, 6)
                            .background(Color(hex: "#FFF6DA"), in: .rect(cornerRadius: 4))
                            .overlay(RoundedRectangle(cornerRadius: 4).strokeBorder(Color(hex: "#1C1C1E"), lineWidth: 2))
                            .padding(.top, 150)
                    }
                }
            }
            .overlay(alignment: .topTrailing) {
                if game.phase == .running || game.phase == .paused {
                    Button(game.phase == .paused ? String(localized: "Continue") : String(localized: "Pause")) {
                        game.phase = game.phase == .paused ? .running : .paused
                        game.last = nil
                        bump += 1
                    }
                    .font(.footnote.weight(.semibold))
                    .padding(.horizontal, 14).padding(.vertical, 8)
                    .glassEffect(.regular.interactive(), in: .capsule)
                    .padding(.top, 18).padding(.trailing, 16)
                }
            }
            .overlay(alignment: .bottom) {
                VStack(spacing: 14) {
                    if let r = result, game.phase == .over {
                        VStack(spacing: 6) {
                            Text("RUN OVER").font(.system(size: 13, weight: .heavy, design: .monospaced)).opacity(0.7)
                            Text("\(r.meters) m · \(r.carrots) 🥕").font(.system(size: 26, weight: .heavy, design: .monospaced))
                            Text("It goes on the group leaderboard.").font(.footnote).foregroundStyle(.secondary)
                        }
                        .padding(16)
                        .frame(maxWidth: 300)
                        .background(.white.opacity(0.85), in: .rect(cornerRadius: 18, style: .continuous))
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                    }
                    Button { game.jump(now: Date().timeIntervalSinceReferenceDate); if game.phase == .running { result = nil }; bump += 1 } label: {
                        Text(game.phase == .running ? String(localized: "JUMP ↑") : (game.phase == .over ? String(localized: "RUN AGAIN") : String(localized: "RUN")))
                            .font(.system(size: 16, weight: .heavy, design: .monospaced))
                            .padding(.horizontal, 26).padding(.vertical, 14)
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(Color(hex: "#1C1C1E"))
                    .glassEffect(.regular.interactive(), in: .capsule)
                    .accessibilityLabel(game.phase == .running ? String(localized: "Jump") : String(localized: "Start run"))
                }
                .padding(.bottom, 26)
            }
            .contentShape(Rectangle())
            .onTapGesture { game.jump(now: Date().timeIntervalSinceReferenceDate); bump += 1 }
        }
        .onAppear {
            if UserDefaults.standard.bool(forKey: "RodaDashAuto") {
                game.auto = true
                game.reset()
                bump += 1
            }
        }
        .animation(.spring(duration: 0.4), value: game.phase == .over)
    }

    private func tick(now: Double, width: Double) -> Bool {
        if game.step(now: now, width: width) {
            let r = (Int(game.distance), game.carrots)
            Task { @MainActor in
                result = r
                bump += 1
                model.callAppTool(item.id, "pet_dash_score", args: "{\"meters\":\(r.0),\"carrots\":\(r.1)}")
            }
        }
        return true
    }

    private func draw(_ ctx: GraphicsContext, size: CGSize, now: Double) {
        let w = size.width, h = size.height
        let ground = h * 0.66
        ctx.fill(Path(CGRect(origin: .zero, size: size)), with: .linearGradient(Gradient(colors: [Color(hex: "#B9C7EE"), Color(hex: "#E9DCE9"), Color(hex: "#F6E7D2")]), startPoint: .zero, endPoint: CGPoint(x: 0, y: ground)))
        // sol
        ctx.fill(Path(ellipseIn: CGRect(x: w * 0.55, y: h * 0.18, width: 54, height: 54)), with: .radialGradient(Gradient(colors: [.white, .white.opacity(0)]), center: CGPoint(x: w * 0.55 + 27, y: h * 0.18 + 27), startRadius: 4, endRadius: 40))
        // morros ao fundo (paralaxe)
        let off = CGFloat((game.distance * 3).truncatingRemainder(dividingBy: 200))
        var hills = Path()
        for i in -1...Int(w / 100) + 2 {
            let x = CGFloat(i) * 100 - off / 2
            hills.addRect(CGRect(x: x, y: ground - 16 - CGFloat((i * 37) % 13), width: 34, height: 40))
        }
        ctx.fill(hills, with: .color(Color(hex: "#9AA0A8").opacity(0.5)))
        // chão
        // Areia embaixo (casa com as abas de vidro) e uma faixa de estrada curta, tracejada.
        ctx.fill(Path(CGRect(x: 0, y: ground, width: w, height: h - ground)), with: .linearGradient(Gradient(colors: [Color(hex: "#E7D3B6"), Color(hex: "#F2DFCD")]), startPoint: CGPoint(x: 0, y: ground), endPoint: CGPoint(x: 0, y: h)))
        ctx.fill(Path(CGRect(x: 0, y: ground, width: w, height: 22)), with: .color(Color(hex: "#4A4A4E")))
        let dashOffset = CGFloat(Int(game.distance * 8) % 28)
        for x in stride(from: -dashOffset, to: w, by: 28) {
            ctx.fill(Path(CGRect(x: x, y: ground + 10, width: 14, height: 2)), with: .color(.white.opacity(0.55)))
        }
        ctx.fill(Path(CGRect(x: 0, y: ground, width: w, height: 3)), with: .color(Color(hex: "#2E2E32")))
        let dashOff = CGFloat((game.distance * 18).truncatingRemainder(dividingBy: 40))
        for i in 0...Int(w / 40) + 1 {
            ctx.fill(Path(CGRect(x: CGFloat(i) * 40 - dashOff, y: ground + 14, width: 12, height: 3)), with: .color(Color(hex: "#5E5E63")))
        }
        let px: CGFloat = 2
        // cactos
        for o in game.obstacles {
            let x = CGFloat(o)
            let c = Color(hex: "#3E8E5A")
            ctx.fill(Path(CGRect(x: x, y: ground - 30, width: 10, height: 30)), with: .color(c))
            ctx.fill(Path(CGRect(x: x - 6, y: ground - 22, width: 6, height: 4)), with: .color(c))
            ctx.fill(Path(CGRect(x: x - 6, y: ground - 28, width: 4, height: 8)), with: .color(c))
            ctx.fill(Path(CGRect(x: x + 10, y: ground - 18, width: 6, height: 4)), with: .color(c))
            ctx.fill(Path(CGRect(x: x + 12, y: ground - 24, width: 4, height: 8)), with: .color(c))
        }
        // cenouras
        for c in game.carrotsX {
            let x = CGFloat(c.x), y = ground - 22 - CGFloat(c.h)
            ctx.fill(Path(CGRect(x: x + 3, y: y, width: 4, height: 4)), with: .color(Color(hex: "#3E8E5A")))
            ctx.fill(Path(CGRect(x: x + 1, y: y + 4, width: 8, height: 5)), with: .color(Color(hex: "#F28C28")))
            ctx.fill(Path(CGRect(x: x + 3, y: y + 9, width: 4, height: 5)), with: .color(Color(hex: "#F28C28")))
        }
        // +1
        for p in game.plusOnes {
            let age = now - p.t
            var t = ctx
            t.opacity = 1 - age / 0.8
            t.draw(Text("+1").font(.system(size: 16, weight: .heavy, design: .monospaced)).foregroundStyle(Color(hex: "#F28C28")), at: CGPoint(x: p.x + 6, y: ground - 40 - p.y - age * 40))
        }
        // jumento
        let rows = (game.phase == .running && game.y == 0 && (Int(now * 10) % 2 == 0)) ? DonkeyArt.sideB : DonkeyArt.sideA
        DonkeyArt.draw(rows, in: ctx, origin: CGPoint(x: 36, y: ground - CGFloat(rows.count) * px - CGFloat(game.y) + 2), px: px)
    }
}
