import SwiftUI
import RodaCore

// MARK: - Globo (projeção ortográfica desenhada no Canvas)

/// Contornos de terra do Natural Earth 1:110m (domínio público), embutidos no app.
enum LandData {
    static let polygons: [[(Double, Double)]] = {
        guard let url = Bundle.main.url(forResource: "land110", withExtension: "json"),
              let data = try? Data(contentsOf: url),
              let flat = try? JSONDecoder().decode([[Double]].self, from: data) else { return [] }
        return flat.map { arr in stride(from: 0, to: arr.count - 1, by: 2).map { (arr[$0], arr[$0 + 1]) } }
    }()
}

struct GeoPoint: Equatable { var lat: Double; var lon: Double }

enum Ortho {
    /// (x, y, z) no globo unitário visto de `center`; z < 0 = lado de trás.
    static func project(_ p: GeoPoint, center c: GeoPoint) -> (Double, Double, Double) {
        let la = p.lat * .pi / 180, lo = p.lon * .pi / 180
        let la0 = c.lat * .pi / 180, lo0 = c.lon * .pi / 180
        let x = cos(la) * sin(lo - lo0)
        let y = cos(la0) * sin(la) - sin(la0) * cos(la) * cos(lo - lo0)
        let z = sin(la0) * sin(la) + cos(la0) * cos(la) * cos(lo - lo0)
        return (x, y, z)
    }

    /// Ponto da tela (normalizado, raio 1) → latitude/longitude. `nil` fora do globo.
    static func inverse(x: Double, y: Double, center c: GeoPoint) -> GeoPoint? {
        let rho = sqrt(x * x + y * y)
        guard rho <= 1 else { return nil }
        if rho < 1e-9 { return c }
        let cc = asin(rho)
        let la0 = c.lat * .pi / 180, lo0 = c.lon * .pi / 180
        let lat = asin(cos(cc) * sin(la0) + y * sin(cc) * cos(la0) / rho)
        let lon = lo0 + atan2(x * sin(cc), rho * cos(cc) * cos(la0) - y * sin(cc) * sin(la0))
        var lonDeg = lon * 180 / .pi
        while lonDeg > 180 { lonDeg -= 360 }
        while lonDeg < -180 { lonDeg += 360 }
        return GeoPoint(lat: lat * 180 / .pi, lon: lonDeg)
    }

    static func slerp(_ a: GeoPoint, _ b: GeoPoint, _ t: Double) -> GeoPoint {
        func vec(_ p: GeoPoint) -> (Double, Double, Double) {
            let la = p.lat * .pi / 180, lo = p.lon * .pi / 180
            return (cos(la) * cos(lo), cos(la) * sin(lo), sin(la))
        }
        let (a1, a2, a3) = vec(a), (b1, b2, b3) = vec(b)
        let d = max(-1, min(1, a1 * b1 + a2 * b2 + a3 * b3))
        let w = acos(d)
        if w < 1e-6 { return a }
        let s1 = sin((1 - t) * w) / sin(w), s2 = sin(t * w) / sin(w)
        let (x, y, z) = (s1 * a1 + s2 * b1, s1 * a2 + s2 * b2, s1 * a3 + s2 * b3)
        return GeoPoint(lat: atan2(z, sqrt(x * x + y * y)) * 180 / .pi, lon: atan2(y, x) * 180 / .pi)
    }
}

struct GlobeStyle {
    static let ocean = [Color(hex: "#1D4E8F"), Color(hex: "#0B2752"), Color(hex: "#050F24")]
    static let land = Color(hex: "#C9B98E")
    static let landEdge = Color(hex: "#8FA36B")
}

/// Cheap stable noise in -1…1 for the inked coastline.
@inline(__always) func inkNoise(_ a: Int, _ b: Int) -> CGFloat {
    let s = sin(Double(a &* 127 &+ b &* 311)) * 43_758.5453
    return CGFloat(s - s.rounded(.down)) * 2 - 1
}

/// `ink`: boil frame for the hand-inked coastline and rim (nil = no ink pass).
func drawGlobe(_ ctx: GraphicsContext, size: CGSize, center: GeoPoint, guess: GeoPoint? = nil, actual: GeoPoint? = nil, arc: Double = 1, pulse: Double = 0, ink: Int? = nil) {
    let r = min(size.width, size.height) / 2 * 0.94
    let c = CGPoint(x: size.width / 2, y: size.height / 2)
    let disc = CGRect(x: c.x - r, y: c.y - r, width: r * 2, height: r * 2)
    // atmosfera
    ctx.fill(Path(ellipseIn: disc.insetBy(dx: -r * 0.06, dy: -r * 0.06)), with: .radialGradient(Gradient(colors: [Color(hex: "#4AA3FF").opacity(0.35), .clear]), center: c, startRadius: r * 0.96, endRadius: r * 1.07))
    // oceano
    ctx.fill(Path(ellipseIn: disc), with: .radialGradient(Gradient(colors: GlobeStyle.ocean), center: CGPoint(x: c.x - r * 0.35, y: c.y - r * 0.4), startRadius: 0, endRadius: r * 1.5))
    func pt(_ p: GeoPoint) -> (CGPoint, Double) {
        var (x, y, z) = Ortho.project(p, center: center)
        if z < 0 { let n = max(1e-9, sqrt(x * x + y * y)); x /= n; y /= n }
        return (CGPoint(x: c.x + r * x, y: c.y - r * y), z)
    }
    // terra
    var land = Path()
    var inked = Path()
    let amp = max(0.5, r * 0.006)
    for (pi, poly) in LandData.polygons.enumerated() {
        var anyFront = false
        var path = Path()
        var inkPath = Path()
        for (i, (lon, lat)) in poly.enumerated() {
            let (p, z) = pt(GeoPoint(lat: lat, lon: lon))
            if z > 0 { anyFront = true }
            if i == 0 { path.move(to: p) } else { path.addLine(to: p) }
            if let f = ink {
                let q = CGPoint(x: p.x + inkNoise(i &+ f &* 977, pi) * amp, y: p.y + inkNoise(pi &+ f &* 613, i) * amp)
                if i == 0 { inkPath.move(to: q) } else { inkPath.addLine(to: q) }
            }
        }
        if anyFront {
            path.closeSubpath(); land.addPath(path)
            if ink != nil { inkPath.closeSubpath(); inked.addPath(inkPath) }
        }
    }
    var clipped = ctx
    clipped.clip(to: Path(ellipseIn: disc))
    clipped.fill(land, with: .linearGradient(Gradient(colors: [Color(hex: "#9DB07A"), GlobeStyle.land, Color(hex: "#D9C8A0")]), startPoint: CGPoint(x: c.x, y: c.y - r), endPoint: CGPoint(x: c.x, y: c.y + r)))
    if ink != nil {
        clipped.stroke(inked, with: .color(Color(hex: "#3B3020").opacity(0.75)), style: StrokeStyle(lineWidth: max(0.9, r * 0.008), lineCap: .round, lineJoin: .round))
    } else {
        clipped.stroke(land, with: .color(GlobeStyle.landEdge.opacity(0.6)), lineWidth: 0.6)
    }
    // graticulado discreto
    var grid = Path()
    for lat in stride(from: -60.0, through: 60, by: 30) {
        var first = true
        for lon in stride(from: -180.0, through: 180, by: 6) {
            let (p, z) = pt(GeoPoint(lat: lat, lon: lon))
            if z > 0 { if first { grid.move(to: p); first = false } else { grid.addLine(to: p) } } else { first = true }
        }
    }
    clipped.stroke(grid, with: .color(.white.opacity(0.06)), lineWidth: 0.5)
    // sombra (luz vindo de cima à esquerda)
    clipped.fill(Path(ellipseIn: disc), with: .radialGradient(Gradient(colors: [.clear, .clear, .black.opacity(0.55)]), center: CGPoint(x: c.x - r * 0.3, y: c.y - r * 0.35), startRadius: 0, endRadius: r * 1.45))
    clipped.fill(Path(ellipseIn: disc), with: .radialGradient(Gradient(colors: [.white.opacity(0.16), .clear]), center: CGPoint(x: c.x - r * 0.42, y: c.y - r * 0.45), startRadius: 0, endRadius: r * 0.7))
    if let f = ink {
        // Hand-inked rim, open like a quick pen loop.
        let side = min(size.width, size.height)
        let rr = r / side
        Ink.render([InkStroke.ellipse(0.5, 0.5, rr * 1.01, rr * 1.01, closed: false, width: 0.009, color: .white.opacity(0.5), span: 0, n: 22)],
                   ctx: ctx, size: size, seed: 5, frame: f, progress: 2, jitter: 0.5)
    }

    // arco palpite → lugar certo
    if let g = guess, let a = actual {
        var path = Path()
        var started = false
        let steps = 48
        for i in 0...Int(Double(steps) * arc) {
            let p = Ortho.slerp(g, a, Double(i) / Double(steps))
            let (sp, z) = pt(p)
            if z > 0 { if !started { path.move(to: sp); started = true } else { path.addLine(to: sp) } }
        }
        ctx.stroke(path, with: .color(.white.opacity(0.85)), style: StrokeStyle(lineWidth: 1.5, dash: [4, 4]))
    }
    if let a = actual {
        let (p, z) = pt(a)
        if z > 0 {
            ctx.fill(Path(ellipseIn: CGRect(x: p.x - 6, y: p.y - 6, width: 12, height: 12)), with: .color(Color(hex: "#FFD60A")))
            ctx.stroke(Path(ellipseIn: CGRect(x: p.x - 6, y: p.y - 6, width: 12, height: 12)), with: .color(.black.opacity(0.4)), lineWidth: 1)
        }
    }
    if let g = guess {
        let (p, z) = pt(g)
        if z > 0 {
            let rr = 14 + pulse * 18
            ctx.stroke(Path(ellipseIn: CGRect(x: p.x - rr, y: p.y - rr, width: rr * 2, height: rr * 2)), with: .color(Color(hex: "#5CE1E6").opacity(1 - pulse)), lineWidth: 1.5)
            ctx.fill(Path(ellipseIn: CGRect(x: p.x - 9, y: p.y - 9, width: 18, height: 18)), with: .radialGradient(Gradient(colors: [Color(hex: "#7FF7FF"), Color(hex: "#18C8D4").opacity(0.3)]), center: p, startRadius: 0, endRadius: 9))
            ctx.stroke(Path(ellipseIn: CGRect(x: p.x - 11, y: p.y - 11, width: 22, height: 22)), with: .color(.white), lineWidth: 2.5)
        }
    }
}

/// Globo girando sozinho (widget) ou parado (ícone).
struct GlobeView: View {
    @Environment(\.ambientPaused) private var ambientPaused
    var spin = true
    var interactive = false
    var center = GeoPoint(lat: 20, lon: 10)
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 30, paused: ambientPaused || !spin || reduceMotion)) { tl in
            let t = tl.date.timeIntervalSinceReferenceDate
            Canvas { ctx, size in
                var c = center
                if spin && !reduceMotion { c.lon = (t * 12).truncatingRemainder(dividingBy: 360) - 180 }
                drawGlobe(ctx, size: size, center: c, ink: reduceMotion ? 0 : Int(t * 10) % 3)
            }
        }
        .accessibilityHidden(true)
    }
}

// MARK: - Folha do MapTap

struct MapTapSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let item: ItemDetail
    let app: AppStateDto

    enum Phase: Equatable { case title, playing, result, finished }
    @State private var phase: Phase = .title
    @State private var center = GeoPoint(lat: 25, lon: 10)
    @State private var dragStart: GeoPoint?
    @State private var guess: GeoPoint?
    @State private var lastResult: (km: Int, points: Int, place: [String: Any], round: Int)?
    @State private var arc: Double = 0
    @State private var pulseStart = Date()

    private var view: AppView { AppView(app) }
    private var places: [[String: Any]] { view.array("places") }
    private var meName: String { model.me?.name ?? String(localized: "You") }
    private var myGuesses: [String: [String: Any]] { (view.dict("guesses")[meName] as? [String: [String: Any]]) ?? [:] }
    private var round: Int { (0..<places.count).first { myGuesses[String($0)] == nil } ?? places.count }
    private var myScore: Int { myGuesses.values.compactMap { ($0["points"] as? NSNumber)?.intValue }.reduce(0, +) }

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            globe
            switch phase {
            case .title: titleScreen
            case .playing, .result: hud
            case .finished: board
            }
        }
        .preferredColorScheme(.dark)
        .overlay(alignment: .topLeading) {
            Button { dismiss() } label: { Image(systemName: "chevron.down").font(.body.weight(.semibold)).frame(width: 36, height: 36) }
                .buttonStyle(.plain).foregroundStyle(.white)
                .glassEffect(.regular.interactive(), in: .circle)
                .padding(.top, 16).padding(.leading, 16)
                .opacity(phase == .title || phase == .finished ? 1 : 0)
                .accessibilityLabel("Close")
        }
        .onAppear {
            if ["maptap-play", "maptap-result"].contains(UserDefaults.standard.string(forKey: "RodaStory") ?? "") { startDemo() }
        }
    }

    private var globe: some View {
        GeometryReader { geo in
            let side = min(geo.size.width, geo.size.height) * (phase == .title ? 0.92 : 1.04)
            TimelineView(.animation(minimumInterval: 1 / 30, paused: phase != .title && guess == nil)) { tl in
                let t = tl.date.timeIntervalSinceReferenceDate
                let pulse = (tl.date.timeIntervalSince(pulseStart) / 1.2).truncatingRemainder(dividingBy: 1)
                Canvas { ctx, size in
                    var c = center
                    if phase == .title { c.lon = (t * 10).truncatingRemainder(dividingBy: 360) - 180 }
                    let actual: GeoPoint? = phase == .result ? lastResult.map { GeoPoint(lat: ($0.place["lat"] as? NSNumber)?.doubleValue ?? 0, lon: ($0.place["lon"] as? NSNumber)?.doubleValue ?? 0) } : nil
                    drawGlobe(ctx, size: size, center: c, guess: guess, actual: actual, arc: arc, pulse: pulse, ink: Int(t * 10) % 3)
                }
            }
            .frame(width: side, height: side)
            .position(x: geo.size.width / 2, y: geo.size.height * (phase == .title ? 0.36 : 0.5))
            .gesture(
                DragGesture(minimumDistance: 6)
                    .onChanged { g in
                        guard phase == .playing else { return }
                        let s = dragStart ?? center
                        if dragStart == nil { dragStart = center }
                        center = GeoPoint(lat: max(-80, min(80, s.lat + g.translation.height / side * 140)), lon: s.lon - g.translation.width / side * 160)
                    }
                    .onEnded { _ in dragStart = nil }
            )
            .simultaneousGesture(
                SpatialTapGesture().onEnded { tap in
                    guard phase == .playing else { return }
                    let r = side / 2 * 0.94
                    let x = (tap.location.x - side / 2) / r, y = -(tap.location.y - side / 2) / r
                    if let p = Ortho.inverse(x: x, y: y, center: center) {
                        withAnimation(.spring(duration: 0.3)) { guess = p }
                        pulseStart = Date()
                        Haptics.selectionTick()
                    }
                }
            )
            .accessibilityLabel("Globe. Drag to spin and tap to place your guess.")
        }
    }

    private var titleScreen: some View {
        VStack(alignment: .leading, spacing: 10) {
            Spacer()
            Text("MapTap").font(.system(size: 56, weight: .regular, design: .serif)).foregroundStyle(.white)
            Text("DAILY GEOGRAPHY GAME").font(.caption.weight(.semibold)).tracking(2).foregroundStyle(.white.opacity(0.6))
            Button { start() } label: { Text(round >= places.count ? String(localized: "See leaderboard") : (round == 0 ? String(localized: "Play") : String(localized: "Continue · round \(round + 1)"))) }
                .buttonStyle(WabiPill())
                .padding(.top, 18)
            Text("The same 5 places for everyone in the group.").font(.caption).foregroundStyle(.white.opacity(0.45))
                .frame(maxWidth: .infinity)
        }
        .padding(.horizontal, 24)
        .padding(.bottom, 34)
    }

    private var hud: some View {
        VStack(spacing: 12) {
            HStack {
                Text("\(meName.uppercased()) · YOUR TURN").font(.caption.weight(.bold)).tracking(1.2)
                Spacer()
                HStack(spacing: 6) {
                    ForEach(0..<max(places.count, 5), id: \.self) { i in
                        Circle().fill(i < round ? Color.white : Color.white.opacity(0.25)).frame(width: 7, height: 7)
                    }
                }
                Spacer()
                Text("\(myScore)").font(.caption.weight(.bold).monospacedDigit())
                    .contentTransition(.numericText())
            }
            .foregroundStyle(.white)
            .padding(.horizontal, 20)
            .padding(.top, 18)

            if round < places.count, phase == .playing {
                let p = places[round]
                VStack(alignment: .leading, spacing: 4) {
                    Text("ROUND \(round + 1) OF \(places.count)").font(.caption2.weight(.bold)).tracking(1.4).foregroundStyle(.white.opacity(0.6))
                    Text(p["name"] as? String ?? "").font(.system(size: 30, design: .serif)).foregroundStyle(.white)
                    Text(p["hint"] as? String ?? "").font(.footnote).foregroundStyle(.white.opacity(0.75))
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(16)
                .glassEffect(.regular, in: .rect(cornerRadius: 22, style: .continuous))
                .padding(.horizontal, 16)
                .transition(.move(edge: .top).combined(with: .opacity))
            }
            Spacer()
            if phase == .playing {
                if guess != nil {
                    Button("Lock it in") { lockIn() }
                        .buttonStyle(WabiPill())
                        .frame(maxWidth: 220)
                        .transition(.scale.combined(with: .opacity))
                } else {
                    Text("Tap the globe where you think it is.").font(.footnote).foregroundStyle(.white.opacity(0.6))
                }
            }
            if phase == .result, let r = lastResult { resultCard(r).transition(.move(edge: .bottom).combined(with: .opacity)) }
        }
        .padding(.bottom, 24)
        .animation(.spring(duration: 0.45, bounce: 0.2), value: phase)
        .animation(.spring(duration: 0.35), value: guess)
    }

    private func resultCard(_ r: (km: Int, points: Int, place: [String: Any], round: Int)) -> some View {
        let country = r.place["country"] as? String ?? ""
        let verdict = r.km < 100 ? String(localized: "Spot on.") : (r.km < 800 ? String(localized: "So close.") : (r.km < 3000 ? String(localized: "Not bad.") : String(localized: "Way off…")))
        let others = view.dict("guesses").filter { $0.key != meName }.compactMap { (k, v) -> (String, Int)? in
            guard let g = (v as? [String: Any])?[String(r.round)] as? [String: Any], let km = (g["km"] as? NSNumber)?.intValue else { return nil }
            return (k, km)
        }.sorted { $0.1 < $1.1 }
        return VStack(alignment: .leading, spacing: 12) {
            Text("\((r.place["name"] as? String ?? "").uppercased()) · \(country.uppercased())").font(.caption2.weight(.bold)).tracking(1.2).foregroundStyle(.white.opacity(0.55))
            Text("\(country). \(verdict)").font(.system(size: 28, design: .serif)).foregroundStyle(.white)
            HStack(spacing: 10) {
                stat(String(localized: "\(r.km.formatted(.number)) km"), String(localized: "AWAY"))
                if let o = others.first {
                    stat(o.0, String(localized: "\(o.1.formatted(.number)) KM · ALREADY PLAYED"))
                } else {
                    stat("\(r.points)", String(localized: "POINTS"))
                }
            }
            Text(r.round + 1 >= places.count ? String(localized: "Last round.") : String(localized: "Your turn.")).font(.footnote).foregroundStyle(.white.opacity(0.7))
            Button(r.round + 1 >= places.count ? String(localized: "See leaderboard") : String(localized: "Next place")) { next() }
                .buttonStyle(WabiPill())
        }
        .padding(18)
        .background(Color(hex: "#151518").opacity(0.92), in: .rect(cornerRadius: 26, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 26, style: .continuous).strokeBorder(.white.opacity(0.08)))
        .padding(.horizontal, 14)
    }

    private func stat(_ big: String, _ small: String) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(big).font(.headline).foregroundStyle(.white).lineLimit(1).minimumScaleFactor(0.7)
            Text(small).font(.caption2.weight(.semibold)).tracking(0.8).foregroundStyle(.white.opacity(0.5)).lineLimit(1).minimumScaleFactor(0.7)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(.white.opacity(0.07), in: .rect(cornerRadius: 14, style: .continuous))
    }

    private var board: some View {
        let rows = view.dict("guesses").map { (k, v) -> BoardRow in
            let g = v as? [String: [String: Any]] ?? [:]
            return BoardRow(name: k, a: g.values.compactMap { ($0["points"] as? NSNumber)?.intValue }.reduce(0, +), b: g.count)
        }.sorted { $0.a > $1.a }
        return VStack(alignment: .leading, spacing: 12) {
            Spacer()
            Text("Group leaderboard").font(.system(size: 34, design: .serif)).foregroundStyle(.white)
            ForEach(Array(rows.enumerated()), id: \.element.id) { i, r in
                HStack(spacing: 12) {
                    Text("\(i + 1)").font(.headline.monospacedDigit()).frame(width: 22)
                    if let p = model.personaNamed(r.name) { Avatar(persona: p, size: 28) }
                    Text(r.name).font(.body.weight(.semibold))
                    Spacer()
                    Text("\(r.a.formatted(.number)) pts").font(.body.monospacedDigit().weight(.bold))
                    Text("\(r.b)/5").font(.caption).foregroundStyle(.white.opacity(0.5))
                }
                .foregroundStyle(.white)
                .padding(12)
                .background(.white.opacity(0.08), in: .rect(cornerRadius: 14, style: .continuous))
            }
            Button("Close") { dismiss() }.buttonStyle(WabiPill()).padding(.top, 8)
        }
        .padding(.horizontal, 20)
        .padding(.bottom, 30)
        .background(LinearGradient(colors: [.clear, .black.opacity(0.85), .black], startPoint: .top, endPoint: .center).ignoresSafeArea())
    }

    // MARK: fluxo

    private func start() {
        Haptics.open()
        withAnimation(.spring(duration: 0.6)) {
            phase = round >= places.count ? .finished : .playing
            center = GeoPoint(lat: 25, lon: 10)
        }
    }

    private func lockIn() {
        guard let g = guess, round < places.count else { return }
        let place = places[round]
        let r = round
        guard let out = model.perform({ try model.core.appCallTool(itemId: item.id, tool: "maptap_guess", argsJson: "{\"round\":\(r),\"lat\":\(g.lat),\"lon\":\(g.lon)}", confirmed: false) }) else { return }
        let json = (try? JSONSerialization.jsonObject(with: Data(out.resultJson.utf8))) as? [String: Any]
        let sc = (json?["structuredContent"] as? [String: Any])?["guesses"] as? [String: Any]
        let mine = (sc?[meName] as? [String: Any])?[String(r)] as? [String: Any]
        let km = (mine?["km"] as? NSNumber)?.intValue ?? 0
        let pts = (mine?["points"] as? NSNumber)?.intValue ?? 0
        km < 300 ? Haptics.commit() : Haptics.action()
        lastResult = (km, pts, place, r)
        arc = 0
        withAnimation(.spring(duration: 0.5)) { phase = .result }
        let target = GeoPoint(lat: (place["lat"] as? NSNumber)?.doubleValue ?? 0, lon: (place["lon"] as? NSNumber)?.doubleValue ?? 0)
        withAnimation(.easeInOut(duration: 1.0)) { center = Ortho.slerp(g, target, 0.5) }
        withAnimation(.easeOut(duration: 1.1).delay(0.2)) { arc = 1 }
    }

    private func next() {
        guess = nil
        lastResult = nil
        withAnimation(.spring(duration: 0.5)) { phase = round >= places.count ? .finished : .playing }
    }

    /// Demonstração (capturas): `-RodaStory maptap-play` marca um palpite; `maptap-result` crava.
    private func startDemo() {
        let story = UserDefaults.standard.string(forKey: "RodaStory") ?? ""
        guard round < places.count else { return }
        phase = .playing
        let p = places[round]
        let target = GeoPoint(lat: (p["lat"] as? NSNumber)?.doubleValue ?? 0, lon: (p["lon"] as? NSNumber)?.doubleValue ?? 0)
        center = GeoPoint(lat: target.lat - 6, lon: target.lon + 8)
        guess = GeoPoint(lat: target.lat + 0.25, lon: target.lon - 0.15)
        pulseStart = Date()
        if story == "maptap-result" {
            Task { @MainActor in
                try? await Task.sleep(for: .seconds(0.8))
                lockIn()
            }
        }
    }
}
