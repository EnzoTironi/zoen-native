import SwiftUI

// MARK: - Confirm in place (Things-style): the button becomes its own confirmation

/// A sheet's bottom row with a destructive icon next to the primary action (Things:
/// [trash] [UPDATE]). Tap the icon and it grows across the row into a red "Remove item"
/// while the primary shrinks into a small ×; tap red to confirm (a check lands, then
/// `confirm` runs), tap × or wait a few seconds and it folds back.
struct ConfirmMorphRow: View {
    let primaryTitle: LocalizedStringKey
    var primaryEnabled = true
    let primary: () -> Void
    var destructiveIcon = "trash"
    let destructiveLabel: LocalizedStringKey
    let confirmTitle: LocalizedStringKey
    let doneTitle: LocalizedStringKey
    let confirm: () -> Void
    var idPrefix = "confirm-row"

    @State private var armed = false
    @State private var done = false
    @State private var disarm: Task<Void, Never>?
    @State private var armedAt = Date.distantPast
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    private let h: CGFloat = 52

    private var spring: Animation {
        reduceMotion ? .easeInOut(duration: 0.2) : .spring(response: 0.38, dampingFraction: 0.78)
    }

    var body: some View {
        HStack(spacing: 10) {
            // The destructive side: an icon chip that grows into the confirmation.
            Button {
                if !armed { arm() } else if !done { fire() }
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: done ? "checkmark" : destructiveIcon)
                        .font(.system(size: 17, weight: .semibold))
                        .contentTransition(.symbolEffect(.replace))
                    if armed {
                        Text(done ? doneTitle : confirmTitle)
                            .font(.body.weight(.semibold))
                            .lineLimit(1)
                            .contentTransition(.opacity)
                            .transition(.opacity.combined(with: .scale(scale: 0.9, anchor: .leading)))
                    }
                }
                .foregroundStyle(armed ? Color.white : Palette.danger)
                .frame(maxWidth: armed ? .infinity : h, minHeight: h)
                .background(armed ? Palette.danger : Palette.danger.opacity(0.12), in: .rect(cornerRadius: 16, style: .continuous))
                .overlay(alignment: .bottom) {
                    if armed && !done { FoldBackBar(seconds: ConfirmTiming.window).padding(.horizontal, 14).padding(.bottom, 6) }
                }
                .contentShape(.rect(cornerRadius: 16))
            }
            .buttonStyle(PressScaleStyle())
            .accessibilityLabel(armed ? Text(confirmTitle) : Text(destructiveLabel))
            .accessibilityIdentifier(armed ? "\(idPrefix)-confirm" : "\(idPrefix)-destructive")

            // The primary side: full width, or a small × while armed.
            Button {
                if armed { cancel() } else { primary() }
            } label: {
                ZStack {
                    if armed {
                        Image(systemName: "xmark")
                            .font(.system(size: 15, weight: .bold))
                            .transition(.opacity.combined(with: .scale(scale: 0.5)))
                    } else {
                        Text(primaryTitle)
                            .font(.body.weight(.semibold))
                            .lineLimit(1)
                            .transition(.opacity)
                    }
                }
                .foregroundStyle(armed ? Palette.textPrimary : Palette.background)
                .frame(maxWidth: armed ? h : .infinity, minHeight: h)
                .background(armed ? Palette.textPrimary.opacity(0.08) : Palette.textPrimary.opacity(primaryEnabled ? 1 : 0.3),
                            in: .rect(cornerRadius: 16, style: .continuous))
                .contentShape(.rect(cornerRadius: 16))
            }
            .buttonStyle(PressScaleStyle())
            .disabled(!armed && !primaryEnabled)
            .accessibilityLabel(armed ? Text("Cancel") : Text(primaryTitle))
            .accessibilityIdentifier(armed ? "\(idPrefix)-cancel" : "\(idPrefix)-primary")
        }
        .animation(spring, value: armed)
        .animation(spring, value: done)
        .onDisappear { disarm?.cancel() }
    }

    private func arm() {
        Haptics.warning()
        armed = true
        armedAt = .now
        disarm?.cancel()
        disarm = Task { @MainActor in
            try? await Task.sleep(for: .seconds(ConfirmTiming.window))
            if !Task.isCancelled, armed, !done {
                Haptics.selectionTick()
                armed = false
            }
        }
    }

    private func cancel() {
        Haptics.selectionTick()
        disarm?.cancel()
        armed = false
    }

    private func fire() {
        // A double tap shouldn't arm and confirm at once: the question has to be seen first.
        guard Date.now.timeIntervalSince(armedAt) > ConfirmTiming.grace else { return }
        disarm?.cancel()
        Haptics.remove()
        done = true
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(reduceMotion ? 150 : 520))
            confirm()
        }
    }
}

/// One destructive row/button that turns into its own confirmation: tap once and a red
/// fill sweeps across it from the leading edge while the label becomes the question; tap
/// again to confirm (a check, then `action`); × or a few seconds folds it back.
/// `compact` is the inline pill (e.g. "Revogar" at the end of a row).
struct ConfirmInPlaceButton: View {
    let title: String
    var systemImage: String?
    let confirmTitle: String
    let doneTitle: String
    var compact = false
    var identifier: String
    let action: () -> Void

    @State private var armed = false
    @State private var done = false
    @State private var disarm: Task<Void, Never>?
    @State private var armedAt = Date.distantPast
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var spring: Animation {
        reduceMotion ? .easeInOut(duration: 0.2) : .spring(response: 0.4, dampingFraction: 0.8)
    }

    var body: some View {
        HStack(spacing: 8) {
            Button {
                if !armed { arm() } else if !done { fire() }
            } label: {
                HStack(spacing: 10) {
                    if let systemImage {
                        Image(systemName: done ? "checkmark" : systemImage)
                            .font(.system(size: compact ? 13 : 16, weight: .semibold))
                            .contentTransition(.symbolEffect(.replace))
                            .frame(width: compact ? nil : 22)
                    }
                    Text(done ? doneTitle : armed ? confirmTitle : title)
                        .font(compact ? .subheadline.weight(.semibold) : .body.weight(armed ? .semibold : .regular))
                        .lineLimit(1)
                        .contentTransition(.interpolate)
                    if !compact { Spacer(minLength: 0) }
                }
                .foregroundStyle(armed ? Color.white : Palette.danger)
                .padding(.horizontal, compact ? 12 : 12)
                .frame(minHeight: compact ? 32 : 46)
                .background(alignment: .leading) {
                    GeometryReader { g in
                        let shape = RoundedRectangle(cornerRadius: compact ? 16 : 14, style: .continuous)
                        ZStack(alignment: .leading) {
                            shape.fill(Palette.danger.opacity(compact && !armed ? 0.1 : 0))
                            shape.fill(Palette.danger)
                                .frame(width: armed ? g.size.width : 0)
                        }
                    }
                }
                .overlay(alignment: .bottom) {
                    if armed && !done && !compact { FoldBackBar(seconds: ConfirmTiming.window).padding(.horizontal, 12).padding(.bottom, 5) }
                }
                .contentShape(.rect)
            }
            // In a List row (compact), only .borderless keeps the row from claiming the tap.
            .modifier(ConfirmButtonStyle(compact: compact))
            .accessibilityLabel(Text(armed ? confirmTitle : title))
            // In a List row the cell keeps the element it first saw, so the compact pill keeps
            // one identifier and says "armed" in its value instead of swapping ids.
            .accessibilityIdentifier(armed && !compact ? "\(identifier)-confirm" : identifier)
            .accessibilityValue(compact && armed ? Text(verbatim: "armed") : Text(verbatim: ""))

            // The compact pill sits in list rows, where a second button in the row muddles
            // the taps: there, not tapping (it folds back on its own) is the cancel.
            if armed && !done && !compact {
                Button { cancel() } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 12, weight: .bold))
                        .foregroundStyle(Palette.textSecondary)
                        .frame(width: compact ? 32 : 40, height: compact ? 32 : 40)
                        .background(Palette.textPrimary.opacity(0.08), in: .circle)
                }
                .buttonStyle(.plain)
                .transition(.scale(scale: 0.4).combined(with: .opacity))
                .accessibilityLabel(Text("Cancel"))
                .accessibilityIdentifier("\(identifier)-cancel")
            }
        }
        .animation(spring, value: armed)
        .animation(spring, value: done)
        .onDisappear { disarm?.cancel() }
    }

    private func arm() {
        Haptics.warning()
        armed = true
        armedAt = .now
        disarm?.cancel()
        disarm = Task { @MainActor in
            try? await Task.sleep(for: .seconds(ConfirmTiming.window))
            if !Task.isCancelled, armed, !done {
                Haptics.selectionTick()
                armed = false
            }
        }
    }

    private func cancel() {
        Haptics.selectionTick()
        disarm?.cancel()
        armed = false
    }

    private func fire() {
        // A double tap shouldn't arm and confirm at once: the question has to be seen first.
        guard Date.now.timeIntervalSince(armedAt) > ConfirmTiming.grace else { return }
        disarm?.cancel()
        Haptics.remove()
        done = true
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(reduceMotion ? 150 : 480))
            action()
        }
    }
}

private struct ConfirmButtonStyle: ViewModifier {
    var compact: Bool
    func body(content: Content) -> some View {
        if compact { content.buttonStyle(.borderless) } else { content.buttonStyle(PressScaleStyle()) }
    }
}

enum ConfirmTiming {
    /// How long an armed confirmation waits before folding back.
    static let window: Double = 5
    /// A second tap sooner than this after arming is the same tap, not a confirmation.
    static let grace: Double = 0.35
}

/// A hairline that drains while an armed confirmation waits, so it's clear it will fold back.
private struct FoldBackBar: View {
    let seconds: Double
    @State private var left: CGFloat = 1
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        GeometryReader { g in
            Capsule().fill(Color.white.opacity(0.45))
                .frame(width: g.size.width * left, height: 2)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(height: 2)
        .opacity(reduceMotion ? 0 : 1)
        .onAppear { withAnimation(.linear(duration: seconds)) { left = 0 } }
        .accessibilityHidden(true)
    }
}
