import SwiftUI
import RodaCore

/// Revisar antes de agir (poster #045): quem propôs, o que acontece, e três
/// escolhas claras. A aprovação fica presa ao hash do conteúdo.
struct RequestReviewView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let requestId: String

    private var request: AgentRequestDto? { model.requests.first { $0.id == requestId } }

    var body: some View {
        ScrollView {
            if let r = request {
                VStack(alignment: .leading, spacing: 18) {
                    VStack(alignment: .leading, spacing: 10) {
                        Text("Proposed by").font(.caption).foregroundStyle(Palette.textSecondary)
                        HStack(spacing: 12) {
                            AgentAvatar(persona: r.agent, size: 44)
                            VStack(alignment: .leading, spacing: 1) {
                                Text("\(r.agent.name) · \(r.agent.isMine ? "seu agente" : "agente de \(r.agent.ownerName ?? "")")")
                                    .font(.subheadline.weight(.semibold))
                                Text(RodaTime.relative(r.openedMs)).font(.caption).foregroundStyle(Palette.textSecondary)
                            }
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .solidCard(radius: 18, padding: 14)

                    Text("\(r.agent.name) suggests the action below and asks for your approval to continue.")
                        .font(.subheadline).foregroundStyle(Palette.textSecondary)

                    VStack(alignment: .leading, spacing: 12) {
                        HStack(alignment: .top, spacing: 12) {
                            Image(systemName: "hand.raised.fill").foregroundStyle(Palette.amber)
                                .frame(width: 40, height: 40)
                                .background(Palette.amber.opacity(0.14), in: .rect(cornerRadius: 12, style: .continuous))
                            VStack(alignment: .leading, spacing: 2) {
                                Text(r.title).font(.headline)
                                Text(r.detail).font(.subheadline).foregroundStyle(Palette.textSecondary)
                            }
                        }
                        Divider().opacity(0.5)
                        detailRow("bubble.left.and.bubble.right", String(localized: "Chat: \(r.spaceTitle)"))
                        detailRow("person.2", String(localized: "Who sees it: \(r.audience)"))
                        detailRow("bolt", String(localized: "Action: \(r.actionLabel)"))
                        if let c = r.costCents { detailRow("brazilianrealsign.circle", String(localized: "Amount: \(Money.format(c))")) }
                        detailRow("exclamationmark.shield", r.reason, tint: Palette.danger)
                    }
                    .solidCard(radius: 18, padding: 14)

                    status(r)

                    if r.status == .pending || r.status == .stale {
                        VStack(spacing: 10) {
                            Button {
                                model.approve(r)
                            } label: {
                                Text("Approve").font(.headline).frame(maxWidth: .infinity, minHeight: 34)
                            }
                            .buttonStyle(.glassProminent)
                            .disabled(r.status == .stale)

                            if let item = r.itemId {
                                Button {
                                    model.go(.item(item))
                                } label: {
                                    Text("Edit in the plan").font(.headline).frame(maxWidth: .infinity, minHeight: 34)
                                }
                                .buttonStyle(.glass)
                            }

                            Button(role: .destructive) {
                                model.deny(r)
                            } label: {
                                Text("Decline").font(.headline).frame(maxWidth: .infinity, minHeight: 34)
                            }
                            .buttonStyle(.glass)
                            .tint(Palette.danger)
                        }
                    }
                }
                .padding(.horizontal, 18)
                .padding(.vertical, 12)
                .frame(maxWidth: 640)
                .frame(maxWidth: .infinity)
            } else {
                InkEmptyState(pose: .roar, title: String(localized: "Request not found"))
            }
        }
        .background(NightBackdrop())
        .navigationTitle("Review request")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
    }

    private func detailRow(_ symbol: String, _ text: String, tint: Color = Palette.textSecondary) -> some View {
        Label {
            Text(text).font(.subheadline)
        } icon: {
            Image(systemName: symbol).foregroundStyle(tint)
        }
    }

    @ViewBuilder
    private func status(_ r: AgentRequestDto) -> some View {
        switch r.status {
        case .stale:
            Label("The content changed after the request. The old approval no longer counts: undo the edit or ask again.", systemImage: "arrow.triangle.2.circlepath")
                .font(.subheadline).foregroundStyle(Palette.amber)
        case .approved:
            Label("Approved. Done as a simulation: no real payment or message.", systemImage: "checkmark.seal.fill")
                .font(.subheadline).foregroundStyle(Palette.success)
        case .denied:
            Label("Declined. Your decision stops the action.", systemImage: "xmark.circle")
                .font(.subheadline).foregroundStyle(Palette.textSecondary)
        case .pending:
            Label("Approval is bound to the content: if someone edits it, it stops counting.", systemImage: "lock.doc")
                .font(.caption).foregroundStyle(Palette.textTertiary)
        }
    }
}
