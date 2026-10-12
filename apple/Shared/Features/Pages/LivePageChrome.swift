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

    func livePageChrome<Status: View, Actions: View>(
        onClose: (() -> Void)? = nil,
        @ViewBuilder status: () -> Status,
        @ViewBuilder actions: () -> Actions
    ) -> some View {
        modifier(LivePageChrome(onClose: onClose, status: status(), actions: actions()))
    }
}

private struct LivePageChrome<Status: View, Actions: View>: ViewModifier {
    let onClose: (() -> Void)?
    let status: Status
    let actions: Actions
    #if os(macOS)
    @FocusState private var closeFocused: Bool
    #endif

    func body(content: Content) -> some View {
        #if os(macOS)
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                status
                Spacer(minLength: 0)
                actions
                if let onClose {
                    Button(action: onClose) {
                        Image(systemName: "xmark").frame(width: 28, height: 28)
                    }
                    .accessibilityLabel(Text("Close details"))
                    .accessibilityIdentifier("mac-inspector-close")
                    .help("Close details (Esc)")
                    .keyboardShortcut(.cancelAction)
                    .focused($closeFocused)
                    .onAppear { closeFocused = true }
                }
            }
            .labelStyle(.iconOnly)
            .buttonStyle(.borderless)
            .foregroundStyle(Palette.textSecondary)
            .frame(minHeight: 28)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(Palette.background)
            content
        }
        #else
        content.toolbar {
            ToolbarItem(placement: .principal) { status }
            ToolbarItemGroup(placement: .primaryAction) { actions }
        }
        #endif
    }
}
