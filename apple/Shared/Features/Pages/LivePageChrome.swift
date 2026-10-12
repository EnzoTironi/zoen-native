import SwiftUI
import RodaCore

struct LivePageMetadata: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let item: ItemDetail
    var version: UInt32? = nil

    var body: some View {
        let displayedVersion = version ?? item.version
        let last = item.versions.first { $0.number == displayedVersion }
        let who = last.map { $0.author.isMe ? String(localized: "you") : $0.author.name } ?? item.createdBy.name
        HStack(spacing: 8) {
            Image(systemName: "doc.richtext").foregroundStyle(Palette.action)
            Text(item.spaceTitle).fontWeight(.medium)
            Text("·")
            if displayedVersion > 0, let last {
                Text("v\(displayedVersion) · \(who) · \(RodaTime.relative(last.atMs))")
                    .contentTransition(.numericText())
            } else if displayedVersion > 0 {
                Text("v\(displayedVersion)")
            } else {
                Text("Not shared yet · \(who)")
            }
            Spacer()
        }
        .font(.caption)
        .foregroundStyle(Palette.textSecondary)
        .padding(.horizontal, 22)
        .padding(.top, 6)
        .animation(reduceMotion ? nil : .spring(duration: 0.4), value: version ?? item.version)
    }
}

private struct LivePageSurface: ViewModifier {
    func body(content: Content) -> some View {
        content
            .frame(maxWidth: 760)
            .frame(maxWidth: .infinity)
            .background(Palette.background.ignoresSafeArea())
    }
}

extension View {
    func livePageSurface() -> some View { modifier(LivePageSurface()) }
}
