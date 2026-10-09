import SwiftUI
import RodaCore

// Privacy labels (end-to-end / Closed / Public) used to live here as PrivacyPill.
// Users don't need that jargon in the chrome; the lock is gone with the pill.

/// Honest "not yet" note (Agents screen). No invented data.
struct PhaseNote: View {
    let symbol: String
    let title: String
    let text: String
    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: symbol).font(.title3).foregroundStyle(Palette.action)
                .frame(width: 40, height: 40)
                .background(Palette.action.opacity(0.12), in: .rect(cornerRadius: 12, style: .continuous))
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.subheadline.weight(.semibold))
                Text(text).font(.caption).foregroundStyle(Palette.textSecondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .solidCard(radius: 18, padding: 14)
    }
}

// MARK: - Medidor de orçamento

struct BudgetRing: View {
    let spent: Int64
    let limit: Int64
    var size: CGFloat = 44
    var lineWidth: CGFloat = 5
    var showsLabel = false

    var fraction: Double { limit > 0 ? min(1, Double(spent) / Double(limit)) : 1 }
    var color: Color { fraction >= 1 ? Palette.danger : (fraction >= 0.8 ? Palette.amber : Palette.success) }

    var body: some View {
        ZStack {
            Circle().stroke(Palette.surfaceMuted, lineWidth: lineWidth)
            Circle()
                .trim(from: 0, to: max(0.02, fraction))
                .stroke(color.gradient, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .animation(.spring(duration: 0.6), value: fraction)
            if showsLabel {
                Text("\(Int((fraction * 100).rounded()))%")
                    .font(.system(size: size * 0.24, weight: .bold, design: .rounded))
                    .monospacedDigit()
                    .contentTransition(.numericText())
            }
        }
        .frame(width: size, height: size)
        .accessibilityLabel("Budget: \(Money.format(spent)) of \(Money.format(limit))")
    }
}

// MARK: - Barra de total do plano

struct PlanBudgetBar: View {
    let total: Int64
    let budget: Int64?

    var body: some View {
        let over = budget.map { total > $0 } ?? false
        VStack(alignment: .leading, spacing: 6) {
            if let budget {
                GeometryReader { geo in
                    let f = min(1, Double(total) / Double(max(budget, 1)))
                    ZStack(alignment: .leading) {
                        Capsule().fill(Palette.surfaceMuted)
                        Capsule()
                            .fill(over ? AnyShapeStyle(Palette.danger.gradient) : AnyShapeStyle(Palette.actionGradient))
                            .frame(width: max(8, geo.size.width * f))
                            .animation(.spring(duration: 0.5), value: total)
                    }
                }
                .frame(height: 6)
            }
            HStack(spacing: 4) {
                Text(Money.format(total)).fontWeight(.semibold).monospacedDigit().contentTransition(.numericText())
                if let budget {
                    Text("of \(Money.format(budget))").foregroundStyle(Palette.textSecondary)
                    Spacer()
                    Text(over ? String(localized: "\(Money.format(total - budget)) over") : String(localized: "\(Money.format(budget - total)) left"))
                        .foregroundStyle(over ? Palette.danger : Palette.textSecondary)
                        .monospacedDigit()
                }
            }
            .font(.footnote)
        }
    }
}

// MARK: - Cartão de Item na conversa (sólido; "nasce" da mensagem do agente)

struct ItemCardView: View {
    let card: ItemCard

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(card.title)
                    .font(.editorial(20))
                    .foregroundStyle(Palette.textPrimary)
                    .multilineTextAlignment(.leading)
                Spacer(minLength: 6)
                Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(Palette.textTertiary)
            }

            if !card.summary.isEmpty {
                Text(card.summary)
                    .font(.subheadline)
                    .foregroundStyle(Palette.textSecondary)
                    .lineLimit(3)
                    .multilineTextAlignment(.leading)
            }

            if let total = card.totalCents {
                PlanBudgetBar(total: total, budget: card.budgetCents)
                HStack(spacing: 12) {
                    Label("\(card.lineCount) items", systemImage: "list.bullet")
                    if card.doneCount > 0 {
                        Label("\(card.doneCount) done", systemImage: "checkmark.circle.fill").foregroundStyle(Palette.success)
                    }
                }
                .font(.caption)
                .foregroundStyle(Palette.textSecondary)
            }
        }
        .frame(maxWidth: 360, alignment: .leading)
        .solidCard(radius: 20, padding: 14)
        .contentShape(.rect)
    }
}

// MARK: - Cartão de pedido

struct RequestCard: View {
    let request: AgentRequestDto
    var compact = false
    var onApprove: () -> Void
    var onDeny: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: icon)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(iconColor)
                    .frame(width: 34, height: 34)
                    .background(iconColor.opacity(0.14), in: .rect(cornerRadius: 10, style: .continuous))
                VStack(alignment: .leading, spacing: 3) {
                    Text(request.title).font(.body.weight(.semibold)).foregroundStyle(Palette.textPrimary)
                    if !compact {
                        Text(request.detail).font(.subheadline).foregroundStyle(Palette.textSecondary).fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer(minLength: 0)
                if let cost = request.costCents {
                    Text(Money.format(cost)).font(.body.weight(.semibold)).monospacedDigit()
                }
            }

            // Audience, kind and chat live on the review page (and in VoiceOver), not as tags.
            Color.clear.frame(height: 0)
                .accessibilityElement()
                .accessibilityLabel("\(request.audience), \(request.actionLabel), \(request.spaceTitle)")

            if request.reason.hasPrefix(String(localized: "Red line")) {
                Label(request.reason, systemImage: "exclamationmark.shield.fill")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(Palette.danger)
            }

            switch request.status {
            case .pending:
                HStack(spacing: 10) {
                    Button(action: onDeny) { Text("Decline").frame(maxWidth: .infinity) }
                        .buttonStyle(.glass)
                    Button(action: onApprove) { Text("Approve").fontWeight(.semibold).frame(maxWidth: .infinity) }
                        .buttonStyle(.glassProminent)
                }
                .controlSize(.large)
            case .stale:
                Label("The plan changed after the request. Approval only covers the exact content.", systemImage: "arrow.triangle.2.circlepath")
                    .font(.footnote).foregroundStyle(Palette.amber)
            case .approved:
                Label("Approved", systemImage: "checkmark.circle.fill").font(.footnote.weight(.semibold)).foregroundStyle(Palette.success)
            case .denied:
                Label("Declined", systemImage: "xmark.circle.fill").font(.footnote.weight(.semibold)).foregroundStyle(Palette.textSecondary)
            }
        }
        .solidCard(radius: 22, padding: 16)
    }

    private var icon: String {
        switch request.actionLabel {
        case String(localized: "Payment"): "creditcard.fill"
        case String(localized: "Send outside"): "paperplane.fill"
        case String(localized: "Publish"): "megaphone.fill"
        case String(localized: "Delete for real"): "trash.fill"
        default: "square.and.pencil"
        }
    }

    private var iconColor: Color {
        request.actionLabel == String(localized: "Payment") ? Palette.success : Palette.action
    }
}

struct Chip: View {
    let text: String
    var symbol: String? = nil
    var body: some View {
        HStack(spacing: 4) {
            if let symbol { Image(systemName: symbol).font(.caption2) }
            Text(text).lineLimit(1)
        }
        .font(.caption.weight(.medium))
        .foregroundStyle(Palette.textSecondary)
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(Palette.surfaceMuted, in: .capsule)
    }
}

// MARK: - Toast (desfazer / reação do agente / erro)

struct ToastModel: Identifiable, Equatable {
    enum Kind: Equatable { case undo(UndoToken), agent(Persona), error, info }
    let id = UUID()
    let kind: Kind
    let text: String
}

struct ToastView: View {
    let toast: ToastModel
    var onUndo: (UndoToken) -> Void
    var onClose: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            switch toast.kind {
            case .undo(let token):
                Image(systemName: "checkmark.circle.fill").foregroundStyle(Palette.success)
                Text(toast.text).font(.subheadline).lineLimit(2)
                Spacer(minLength: 4)
                Button {
                    onUndo(token)
                } label: {
                    Label("Undo", systemImage: "arrow.uturn.backward").font(.subheadline.weight(.semibold))
                }
                .buttonStyle(.glassProminent)
            case .agent(let p):
                AgentAvatar(persona: p, size: 28, showsOwner: false)
                Text(toast.text).font(.subheadline).lineLimit(3)
                Spacer(minLength: 0)
            case .error:
                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(Palette.amber)
                Text(toast.text).font(.subheadline).lineLimit(3)
                Spacer(minLength: 0)
            case .info:
                Image(systemName: "info.circle.fill").foregroundStyle(Palette.action)
                Text(toast.text).font(.subheadline).lineLimit(3)
                Spacer(minLength: 0)
            }
        }
        .padding(.leading, 16)
        .padding(.trailing, 8)
        .padding(.vertical, 8)
        .frame(minHeight: 52)
        .glassEffect(.regular, in: .capsule)
        .padding(.horizontal, 16)
        .onTapGesture(perform: onClose)
    }
}

// MARK: - Pílulas de filtro

struct FilterPills<T: Hashable & Identifiable>: View {
    let options: [T]
    @Binding var selection: T
    let label: (T) -> String
    var inset: CGFloat = 16
    @Namespace private var pill

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(options) { o in
                    let on = o == selection
                    Button {
                        guard !on else { return }
                        Haptics.selectionTick()
                        withAnimation(.spring(duration: 0.35, bounce: 0.25)) { selection = o }
                    } label: {
                        // Selected: a solid brand capsule that slides between chips; the rest are glass.
                        Text(label(o))
                            .font(.subheadline.weight(on ? .bold : .medium))
                            .foregroundStyle(on ? Color.white : Palette.textPrimary)
                            .padding(.horizontal, 16)
                            .frame(height: 36)
                            .background {
                                if on {
                                    Capsule().fill(Palette.action)
                                        .matchedGeometryEffect(id: "selected", in: pill)
                                        .shadow(color: Palette.action.opacity(0.3), radius: 6, y: 3)
                                } else {
                                    Capsule().fill(.clear).glassEffect(.regular.interactive(), in: .capsule)
                                }
                            }
                            .contentShape(.capsule)
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(on ? .isSelected : [])
                }
            }
            .padding(.horizontal, inset)
            .padding(.vertical, 4)
        }
        .scrollClipDisabled()
    }
}

// MARK: - Indicador "pensando"

struct ThinkingDots: View {
    @Environment(\.ambientPaused) private var ambientPaused
    var body: some View {
        TimelineView(.animation(paused: ambientPaused)) { ctx in
            let t = ctx.date.timeIntervalSinceReferenceDate * 4
            HStack(spacing: 5) {
                ForEach(0..<3) { i in
                    Circle()
                        .fill(Palette.textSecondary)
                        .frame(width: 7, height: 7)
                        .opacity(0.3 + 0.7 * max(0, sin(t - Double(i) * 0.8)))
                }
            }
        }
    }
}
