import SwiftUI

/// Ajustes: the pieces every settings screen is made of. A screen is just a list of
/// groups, so each new screen stays a few lines (see `SettingsScreens.swift`).
///
/// Look: neutral grouped cards, one 48pt back circle and one 48pt close circle, and
/// control outlines in `controlOutline` (≥ 3:1 against the card in light and dark).
enum SettingsStyle {
    static let card = Color.adaptive(light: "#F3F4F7", dark: "#151A24")
    static let cardEdge = Palette.hairline
    /// Rings, field and button outlines: 3.45:1 on the light card (3.8:1 on white),
    /// 3.9:1 on the dark card.
    static let controlOutline = Color.adaptive(light: "#7A8396", dark: "#6B7790")
    static let radius: CGFloat = 16
    static let rowHeight: CGFloat = 52
    static let button: CGFloat = 48
}

/// Close for the whole Ajustes stack (the X). Back pops one screen.
struct SettingsCloseAction {
    let close: () -> Void
    func callAsFunction() { close() }
}

private struct SettingsCloseKey: EnvironmentKey {
    static var defaultValue: SettingsCloseAction { SettingsCloseAction(close: {}) }
}

extension EnvironmentValues {
    var settingsClose: SettingsCloseAction {
        get { self[SettingsCloseKey.self] }
        set { self[SettingsCloseKey.self] = newValue }
    }
}

// MARK: - Page

/// A settings screen: our own top bar (back · title · close) over a scroll of groups.
struct SettingsPage<Content: View, Footer: View>: View {
    let title: LocalizedStringKey
    var showsBack = true
    @ViewBuilder var content: Content
    @ViewBuilder var footer: Footer

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) { content }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 24)
                .frame(maxWidth: 560)
                .frame(maxWidth: .infinity)
        }
        .scrollDismissesKeyboard(.interactively)
        .safeAreaInset(edge: .top, spacing: 0) { SettingsTopBar(title: title, showsBack: showsBack) }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            footer
                .padding(.horizontal, 16)
                .padding(.bottom, 8)
                .frame(maxWidth: 560)
        }
        .background(Palette.background.ignoresSafeArea())
        #if os(iOS)
        .toolbar(.hidden, for: .navigationBar)
        #endif
    }
}

extension SettingsPage where Footer == EmptyView {
    init(title: LocalizedStringKey, showsBack: Bool = true, @ViewBuilder content: () -> Content) {
        self.title = title
        self.showsBack = showsBack
        self.content = content()
        self.footer = EmptyView()
    }
}

struct SettingsTopBar: View {
    let title: LocalizedStringKey
    var showsBack = true
    @Environment(\.dismiss) private var dismiss
    @Environment(\.settingsClose) private var close

    var body: some View {
        HStack(spacing: 8) {
            if showsBack {
                SettingsCircleButton(symbol: "chevron.left", label: "Back", id: "settingsBack") {
                    Haptics.tap()
                    dismiss()  // pops exactly one screen of the Ajustes stack
                }
            }
            Text(title)
                .font(.title3.weight(.semibold))
                .foregroundStyle(Palette.textPrimary)
                .lineLimit(1)
                .accessibilityAddTraits(.isHeader)
            Spacer(minLength: 8)
            SettingsCircleButton(symbol: "xmark", label: "Close", id: "settingsClose") {
                Haptics.dismiss()
                close()
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(Palette.background)
    }
}

/// The unified 48pt back / close circle.
struct SettingsCircleButton: View {
    let symbol: String
    let label: LocalizedStringKey
    let id: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 17, weight: .semibold))
                .foregroundStyle(Palette.textPrimary)
                .frame(width: SettingsStyle.button, height: SettingsStyle.button)
                .background(SettingsStyle.card, in: .circle)
                .overlay(Circle().strokeBorder(SettingsStyle.cardEdge, lineWidth: 0.5))
                .contentShape(.circle)
        }
        .buttonStyle(PressScale())
        .accessibilityLabel(Text(label))
        .accessibilityIdentifier(id)
    }
}

/// Press scales to 0.96 over 150ms ease-out; never when disabled or under Reduce Motion.
struct PressScale: ButtonStyle {
    @Environment(\.isEnabled) private var enabled
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed && enabled && !reduceMotion ? 0.96 : 1)
            .opacity(configuration.isPressed && reduceMotion ? 0.7 : 1)
            .animation(.easeOut(duration: 0.15), value: configuration.isPressed)
    }
}

// MARK: - Groups and rows

struct SettingsGroup<Content: View>: View {
    var header: LocalizedStringKey? = nil
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let header {
                Text(header)
                    .font(.footnote.weight(.medium))
                    .foregroundStyle(Palette.textSecondary)
                    .padding(.leading, 4)
                    .accessibilityAddTraits(.isHeader)
            }
            VStack(spacing: 0) {
                Group(subviews: content) { rows in
                    ForEach(Array(rows.enumerated()), id: \.offset) { i, row in
                        if i > 0 { Divider().overlay(SettingsStyle.cardEdge).padding(.leading, 16) }
                        row
                    }
                }
            }
            .background(SettingsStyle.card, in: .rect(cornerRadius: SettingsStyle.radius, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: SettingsStyle.radius, style: .continuous).strokeBorder(SettingsStyle.cardEdge, lineWidth: 0.5))
        }
    }
}

/// Text under a group: what the setting does, in plain words.
struct SettingsCaption: View {
    let text: LocalizedStringKey
    init(_ text: LocalizedStringKey) { self.text = text }
    var body: some View {
        Text(text)
            .font(.footnote)
            .foregroundStyle(Palette.textSecondary)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 4)
    }
}

/// Label, optional detail line, optional value, optional chevron. Tappable when it has an action.
struct SettingsRow: View {
    var symbol: String? = nil
    let title: LocalizedStringKey
    var subtitle: String? = nil
    var value: String? = nil
    var tint: Color = Palette.textPrimary
    var chevron = true
    var id: String? = nil
    var action: (() -> Void)? = nil

    var body: some View {
        if let action {
            Button { Haptics.tap(); action() } label: { label }
                .buttonStyle(RowPress())
                .accessibilityIdentifier(id ?? "")
        } else {
            label.accessibilityElement(children: .combine)
                .accessibilityIdentifier(id ?? "")
        }
    }

    private var label: some View {
        HStack(spacing: 12) {
            if let symbol {
                Image(systemName: symbol)
                    .font(.system(size: 16, weight: .medium))
                    .foregroundStyle(tint == Palette.textPrimary ? Palette.textSecondary : tint)
                    .frame(width: 22)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.body.weight(.medium)).foregroundStyle(tint)
                if let subtitle {
                    Text(subtitle).font(.footnote).foregroundStyle(Palette.textSecondary)
                }
            }
            Spacer(minLength: 8)
            if let value {
                Text(value).font(.subheadline).foregroundStyle(Palette.textSecondary)
                    .multilineTextAlignment(.trailing)
            }
            if chevron && action != nil {
                Image(systemName: "chevron.right")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Palette.textTertiary)
            }
        }
        .padding(.horizontal, 16)
        .frame(minHeight: SettingsStyle.rowHeight)
        .padding(.vertical, subtitle == nil ? 0 : 6)
        .contentShape(.rect)
    }
}

/// Rows highlight instantly (high-frequency: no motion).
struct RowPress: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(configuration.isPressed ? Palette.textPrimary.opacity(0.06) : .clear)
    }
}

/// One choice of many: the ring-with-dot marker from Tema. Selection is also carried by
/// the accessibility trait, not only by the dot.
struct SettingsRadioRow: View {
    let title: LocalizedStringKey
    var subtitle: LocalizedStringKey? = nil
    let selected: Bool
    var id: String? = nil
    let action: () -> Void

    var body: some View {
        Button {
            if !selected { Haptics.selectionTick() }
            action()
        } label: {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.body.weight(.medium)).foregroundStyle(Palette.textPrimary)
                    if let subtitle { Text(subtitle).font(.footnote).foregroundStyle(Palette.textSecondary) }
                }
                Spacer()
                RadioMark(on: selected)
            }
            .padding(.horizontal, 16)
            .frame(minHeight: SettingsStyle.rowHeight)
            .contentShape(.rect)
        }
        .buttonStyle(RowPress())
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier(id ?? "")
    }
}

struct RadioMark: View {
    let on: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var body: some View {
        ZStack {
            Circle().strokeBorder(on ? Palette.textPrimary : SettingsStyle.controlOutline, lineWidth: 1.5)
            Circle().fill(Palette.textPrimary)
                .frame(width: 10, height: 10)
                .scaleEffect(on ? 1 : (reduceMotion ? 1 : 0.25))
                .opacity(on ? 1 : 0)
        }
        .frame(width: 20, height: 20)
        .animation(.spring(duration: 0.3, bounce: 0), value: on)
    }
}

/// The wide black (white in dark) primary action at the bottom of a screen.
struct SettingsPrimaryButton: View {
    let title: LocalizedStringKey
    var id: String? = nil
    var busy = false
    let action: () -> Void
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        Button(action: action) {
            ZStack {
                Text(title).opacity(busy ? 0 : 1)
                if busy { ProgressView().tint(Palette.background) }
            }
            .font(.body.weight(.semibold))
            .foregroundStyle(Palette.background)
            .frame(maxWidth: .infinity, minHeight: 50)
            .background(Palette.textPrimary.opacity(enabled ? 1 : 0.35), in: .capsule)
        }
        .buttonStyle(PressScale())
        .accessibilityIdentifier(id ?? "")
    }
}

/// Honest empty state: says what will show up here and how to get it.
struct SettingsEmpty: View {
    let symbol: String
    let title: LocalizedStringKey
    let detail: LocalizedStringKey
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shown = false

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: symbol)
                .font(.system(size: 22, weight: .medium))
                .foregroundStyle(Palette.textSecondary)
                .frame(width: 48, height: 48)
                .background(SettingsStyle.card, in: .circle)
            Text(title).font(.headline).foregroundStyle(Palette.textPrimary)
            Text(detail).font(.subheadline).foregroundStyle(Palette.textSecondary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 40)
        .padding(.horizontal, 24)
        .opacity(shown ? 1 : 0)
        .offset(y: shown || reduceMotion ? 0 : 12)
        .blur(radius: shown || reduceMotion ? 0 : 4)
        .onAppear { withAnimation(.easeOut(duration: 0.3)) { shown = true } }
        .accessibilityElement(children: .combine)
    }
}

/// A big number with a label and a progress bar (credits, invites).
struct SettingsMeter: View {
    let label: LocalizedStringKey
    let value: String
    let progress: Double
    let detail: String
    @State private var drawn = 0.0

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label).font(.footnote).foregroundStyle(Palette.textSecondary)
            Text(value).font(.system(size: 32, weight: .medium)).monospacedDigit().foregroundStyle(Palette.textPrimary)
            GeometryReader { g in
                Capsule().fill(Palette.textPrimary.opacity(0.1))
                    .overlay(alignment: .leading) {
                        Capsule().fill(Palette.textPrimary).frame(width: max(6, g.size.width * drawn))
                    }
            }
            .frame(height: 6)
            .accessibilityHidden(true)
            Text(detail).font(.footnote).foregroundStyle(Palette.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(SettingsStyle.card, in: .rect(cornerRadius: SettingsStyle.radius, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: SettingsStyle.radius, style: .continuous).strokeBorder(SettingsStyle.cardEdge, lineWidth: 0.5))
        .accessibilityElement(children: .combine)
        .onAppear { withAnimation(.spring(duration: 0.6, bounce: 0).delay(0.1)) { drawn = min(max(progress, 0), 1) } }
    }
}
