import SwiftUI
import RodaCore

/// The group's countdown (a trip, a show): hand-drawn scene, the time left ticking, the date.
struct CountdownSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let item: ItemDetail
    let app: AppStateDto

    var body: some View {
        let v = AppView(app)
        let target = Date(timeIntervalSince1970: Double(v.int("target")) / 1000)
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    ZStack(alignment: .bottomLeading) {
                        LinearGradient(colors: [Color(hex: "#F9B67A"), Color(hex: "#E9846B"), Color(hex: "#3B6C8F")], startPoint: .top, endPoint: .bottom)
                        DoodleView(doodle: .trip, drawOn: 1.4)
                            .frame(width: 260, height: 260)
                            .frame(maxWidth: .infinity, alignment: .trailing)
                            .offset(x: 30, y: 40)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(v.string("place") ?? item.title)
                                .font(.system(size: 34, weight: .heavy, design: .rounded))
                            Text(target, format: .dateTime.weekday(.wide).day().month(.wide))
                                .font(.headline)
                                .opacity(0.9)
                        }
                        .foregroundStyle(.white)
                        .shadow(color: .black.opacity(0.2), radius: 8, y: 2)
                        .padding(20)
                    }
                    .frame(height: 280)
                    .clipShape(.rect(cornerRadius: 30, style: .continuous))

                    TimelineView(.periodic(from: .now, by: 1)) { ctx in
                        let left = max(0, Int(target.timeIntervalSince(ctx.date)))
                        HStack(spacing: 10) {
                            unit(left / 86_400, String(localized: "days"))
                            unit(left % 86_400 / 3_600, String(localized: "hours"))
                            unit(left % 3_600 / 60, String(localized: "min"))
                            unit(left % 60, String(localized: "sec"))
                        }
                    }

                    if let last = app.lastAction {
                        Text(last).font(.footnote).foregroundStyle(Palette.textTertiary)
                    }

                    let onHome = model.isOnHome(item.id)
                    Button {
                        Haptics.tap()
                        withAnimation(.spring(duration: 0.4)) { onHome ? model.unpinFromHome(item.id) : model.pinToHome(item.id) }
                    } label: {
                        Label(onHome ? "Unpin from Home" : "Pin to Home", systemImage: onHome ? "pin.slash" : "pin")
                            .font(.headline)
                            .frame(maxWidth: .infinity, minHeight: 50)
                    }
                    .buttonStyle(.glass)
                }
                .padding(20)
            }
            .background(Palette.background)
            .navigationTitle(item.title)
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button { dismiss() } label: { Image(systemName: "chevron.down") }.accessibilityLabel("Close")
                }
            }
        }
    }

    private func unit(_ n: Int, _ label: String) -> some View {
        VStack(spacing: 2) {
            Text("\(n)")
                .font(.system(size: 34, weight: .heavy, design: .rounded))
                .monospacedDigit()
                .contentTransition(.numericText(countsDown: true))
                .animation(.spring(duration: 0.4), value: n)
            Text(label).font(.caption.weight(.semibold)).foregroundStyle(Palette.textSecondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 14)
        .background(Palette.surfaceMuted.opacity(0.7), in: .rect(cornerRadius: 18, style: .continuous))
        .accessibilityElement(children: .combine)
    }
}
