import SwiftUI
import RodaCore

/// Tapping the chat's title grows the header into a menu, Slack-style. The avatar still
/// opens the profile (or the people, in a group).
/// The menu that drops out of the header: a line about the chat, then the rows.
struct HeaderMenuPanel: View {
    enum Pick: CaseIterable, Identifiable {
        case members, files, pages, agents, mute, settings
        var id: Self { self }
        var title: String {
            switch self {
            case .members: String(localized: "Members")
            case .files: String(localized: "Files")
            case .pages: String(localized: "Pages")
            case .agents: String(localized: "Agents")
            case .mute: String(localized: "Mute")
            case .settings: String(localized: "Settings")
            }
        }
        var icon: String {
            switch self {
            case .members: "person.2"
            case .files: "doc"
            case .pages: "doc.richtext"
            case .agents: "sparkles"
            case .mute: "bell.slash"
            case .settings: "gearshape"
            }
        }
    }

    let space: SpaceSummary
    let about: String
    let onPick: (Pick) -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shown = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(about)
                .font(.footnote)
                .foregroundStyle(Palette.textSecondary)
                .lineLimit(2)
                .padding(.horizontal, 18)
                .padding(.top, 14)
                .padding(.bottom, 8)
                .opacity(shown ? 1 : 0)
            ForEach(Array(Pick.allCases.enumerated()), id: \.element) { i, pick in
                if pick == .mute { Divider().padding(.leading, 54).padding(.vertical, 2) }
                Button { Haptics.selectionTick(); onPick(pick) } label: {
                    HStack(spacing: 14) {
                        Image(systemName: pick.icon)
                            .font(.system(size: 16, weight: .medium))
                            .frame(width: 24)
                            .foregroundStyle(Palette.textPrimary)
                        Text(pick.title)
                            .font(.body)
                            .foregroundStyle(Palette.textPrimary)
                        Spacer(minLength: 0)
                        if pick != .mute {
                            Image(systemName: "chevron.right")
                                .font(.system(size: 12, weight: .semibold))
                                .foregroundStyle(Palette.textTertiary)
                        }
                    }
                    .padding(.horizontal, 18)
                    .frame(height: 46)
                    .contentShape(.rect)
                }
                .buttonStyle(PressScaleStyle())
                .accessibilityIdentifier("header-menu-\(String(describing: pick))")
                // Rows fall in one after another, like the menu unrolling from the title.
                .opacity(shown ? 1 : 0)
                .offset(y: shown || reduceMotion ? 0 : -10 - CGFloat(i) * 4)
                .animation(reduceMotion ? .easeOut(duration: 0.15)
                           : .spring(response: 0.36, dampingFraction: 0.82).delay(0.03 * Double(i)), value: shown)
            }
            Spacer().frame(height: 8)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassEffect(.regular, in: .rect(cornerRadius: 26, style: .continuous))
        .padding(.horizontal, 12)
        .onAppear { shown = true }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("header-menu")
    }
}
