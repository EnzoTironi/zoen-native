import SwiftUI
import RodaCore

struct RecipeSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let item: ItemDetail
    let app: AppStateDto
    @State private var cooking = false

    var body: some View {
        let v = AppView(app)
        let servings = v.int("servings")
        let base = max(1, v.int("baseServings"))
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    ZStack {
                        LinearGradient(colors: [Color(hex: "#FBE3C8"), Color(hex: "#F2A66E")], startPoint: .top, endPoint: .bottom)
                        DoodleView(doodle: .pot, drawOn: 1.4).frame(width: 170, height: 170)
                    }
                    .frame(height: 210)
                    .clipShape(.rect(cornerRadius: 26, style: .continuous))

                    VStack(alignment: .leading, spacing: 4) {
                        Text(v.string("title") ?? item.title).font(.system(size: 28, weight: .semibold, design: .serif))
                        Text(v.string("subtitle") ?? "").font(.subheadline).foregroundStyle(Palette.textSecondary)
                    }

                    HStack {
                        Text("Servings").font(.headline)
                        Spacer()
                        HStack(spacing: 0) {
                            Button { set(servings - 1) } label: { Image(systemName: "minus").frame(width: 44, height: 40) }
                                .disabled(servings <= 1)
                                .accessibilityLabel("One less serving")
                            Text("\(servings)").font(.headline.monospacedDigit()).frame(minWidth: 30).contentTransition(.numericText())
                            Button { set(servings + 1) } label: { Image(systemName: "plus").frame(width: 44, height: 40) }
                                .disabled(servings >= 12)
                                .accessibilityLabel("One more serving")
                        }
                        .buttonStyle(.plain)
                        .background(Palette.surfaceMuted, in: .capsule)
                    }

                    VStack(alignment: .leading, spacing: 0) {
                        Text("Ingredients").font(.headline).padding(.bottom, 6)
                        ForEach(v.array("ingredients").indices, id: \.self) { i in
                            let ing = v.array("ingredients")[i]
                            let done = (ing["done"] as? NSNumber)?.boolValue ?? false
                            let qty = ((ing["qty"] as? NSNumber)?.doubleValue ?? 0) * Double(servings) / Double(base)
                            Button { check(ing["id"] as? String ?? "") } label: {
                                HStack(spacing: 12) {
                                    RoundedRectangle(cornerRadius: 5, style: .continuous)
                                        .strokeBorder(done ? Color.clear : Palette.textTertiary, lineWidth: 1.6)
                                        .background(RoundedRectangle(cornerRadius: 5, style: .continuous).fill(done ? Color(hex: "#111113") : .clear))
                                        .overlay { if done { Image(systemName: "checkmark").font(.caption.weight(.heavy)).foregroundStyle(.white) } }
                                        .frame(width: 22, height: 22)
                                    Text(ing["name"] as? String ?? "").strikethrough(done).foregroundStyle(done ? Palette.textTertiary : Palette.textPrimary)
                                    Spacer()
                                    Text(Self.amount(qty, unit: ing["unit"] as? String ?? "")).font(.subheadline.monospacedDigit()).foregroundStyle(Palette.textSecondary)
                                        .contentTransition(.numericText())
                                }
                                .padding(.vertical, 11)
                            }
                            .buttonStyle(.plain)
                            Divider()
                        }
                    }

                    if let last = v.log.first {
                        Text("\(last.who) \(last.what)").font(.footnote).foregroundStyle(Palette.textSecondary)
                    }

                    Button { Haptics.open(); cooking = true } label: { Label("Start cooking", systemImage: "frying.pan.fill") }
                        .buttonStyle(WabiPill(primary: true))
                }
                .padding(.horizontal, 18)
                .padding(.bottom, 30)
                .animation(.spring(duration: 0.4), value: servings)
            }
            .background(Palette.background)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button { dismiss() } label: { Image(systemName: "chevron.down") }.accessibilityLabel("Close")
                }
            }
            .navigationDestination(isPresented: $cooking) { CookingMode(item: item, app: app) }
        }
        .onAppear { if UserDefaults.standard.string(forKey: "RodaStory") == "recipe-cook" { cooking = true } }
    }

    private func set(_ n: Int) {
        Haptics.selectionTick()
        model.callAppTool(item.id, "recipe_servings", args: "{\"servings\":\(n)}")
    }

    private func check(_ id: String) {
        Haptics.action()
        model.callAppTool(item.id, "recipe_check", args: "{\"id\":\"\(id)\"}")
    }

    static func amount(_ q: Double, unit: String) -> String {
        let whole = Int(q), frac = q - Double(whole)
        let f: String = frac < 0.13 ? "" : frac < 0.38 ? "¼" : frac < 0.63 ? "½" : frac < 0.88 ? "¾" : ""
        let w = frac >= 0.88 ? whole + 1 : whole
        let num = w == 0 && !f.isEmpty ? f : "\(w)\(f)"
        if unit == "ml" { return "\(Int((q / 10).rounded()) * 10) ml" }
        // Units are recipe data, in the language the recipe was saved in.
        let plural = q > 1.13 ? (unit == "colher" ? "colheres" : unit == "maço" ? "maços" : unit == "bunch" ? "bunches" : unit) : unit
        return "\(num) \(plural)"
    }
}

struct CookingMode: View {
    @Environment(AppModel.self) private var model
    let item: ItemDetail
    let app: AppStateDto
    @State private var step = 0
    @State private var timerEnd: Date?
    @State private var timerTotal: Double = 0

    var body: some View {
        let steps = AppView(app).array("steps")
        let s = steps.indices.contains(step) ? steps[step] : [:]
        let minutes = (s["minutes"] as? NSNumber)?.intValue ?? 0
        VStack(alignment: .leading, spacing: 22) {
            Text("STEP \(step + 1) OF \(steps.count)").font(.caption.weight(.bold)).tracking(1.6).foregroundStyle(Palette.textSecondary)
            Text(s["text"] as? String ?? "").font(.system(size: 30, weight: .semibold, design: .serif)).fixedSize(horizontal: false, vertical: true)
                .contentTransition(.opacity)
            if minutes > 0 {
                Button { startTimer(minutes) } label: { Label("Set \(minutes) minute timer", systemImage: "timer") }
                    .buttonStyle(WabiPill())
                    .frame(maxWidth: 260)
            }
            Spacer()
            HStack(spacing: 10) {
                Button("Back") { go(step - 1, total: steps.count) }.buttonStyle(WabiPill()).disabled(step == 0).opacity(step == 0 ? 0.4 : 1)
                Button(step + 1 >= steps.count ? String(localized: "Finish") : String(localized: "Next")) { go(step + 1, total: steps.count) }.buttonStyle(WabiPill(primary: true))
            }
        }
        .padding(22)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.background)
        .safeAreaInset(edge: .bottom) {
            if let end = timerEnd {
                TimelineView(.periodic(from: .now, by: 1)) { tl in
                    let left = max(0, end.timeIntervalSince(tl.date))
                    HStack(spacing: 14) {
                        ZStack {
                            Circle().stroke(Palette.surfaceMuted, lineWidth: 5)
                            Circle().trim(from: 0, to: timerTotal > 0 ? left / timerTotal : 0).stroke(Color(hex: "#F28C28"), style: StrokeStyle(lineWidth: 5, lineCap: .round)).rotationEffect(.degrees(-90))
                        }
                        .frame(width: 40, height: 40)
                        VStack(alignment: .leading, spacing: 1) {
                            Text(String(format: "%d:%02d", Int(left) / 60, Int(left) % 60)).font(.title2.monospacedDigit().weight(.bold))
                            Text(left == 0 ? String(localized: "Done!") : String(localized: "Kitchen timer")).font(.caption).foregroundStyle(Palette.textSecondary)
                        }
                        Spacer()
                        Button { timerEnd = nil } label: { Image(systemName: "xmark").frame(width: 34, height: 34) }
                            .buttonStyle(.plain).glassEffect(.regular.interactive(), in: .circle)
                            .accessibilityLabel("Stop timer")
                    }
                    .padding(14)
                    .glassEffect(.regular, in: .rect(cornerRadius: 22, style: .continuous))
                    .padding(.horizontal, 14)
                    .padding(.bottom, 6)
                }
                .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .navigationTitle("Cooking mode")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .animation(.spring(duration: 0.4), value: step)
        .animation(.spring(duration: 0.4), value: timerEnd)
        .onAppear {
            if UserDefaults.standard.string(forKey: "RodaStory") == "recipe-cook" {
                step = 1
                startTimer(8)
                timerEnd = Date().addingTimeInterval(8 * 60 - 74)
            }
        }
    }

    private func startTimer(_ minutes: Int) {
        Haptics.commit()
        timerTotal = Double(minutes * 60)
        timerEnd = Date().addingTimeInterval(timerTotal)
    }

    private func go(_ n: Int, total: Int) {
        Haptics.selectionTick()
        if n >= total {
            model.callAppTool(item.id, "recipe_cook", args: "{\"step\":\(total)}")
            step = total - 1
            return
        }
        step = max(0, n)
        model.callAppTool(item.id, "recipe_cook", args: "{\"step\":\(step)}")
    }
}
