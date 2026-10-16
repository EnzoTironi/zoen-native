import SwiftUI
import ZoenCore

/// Agents are citizens: a status (active, paused, revoked) only their owner can read
/// and change on the relay. Views ask through here so the list and the profile agree.
@MainActor @Observable
final class AgentStatuses {
    /// Last status the relay gave for each of your agents; missing = not known yet (offline).
    private(set) var known: [String: AgentStatus] = [:]
    private(set) var working: Set<String> = []

    func load(_ ids: [String], core: ZoenEngine) async {
        for id in ids {
            if let s = try? await core.agentStatus(agentId: id) { known[id] = s }
        }
    }

    /// Pause, resume or revoke; the status only changes once the relay agreed.
    func control(_ id: String, _ action: AgentControl, model: AppModel) async {
        guard !working.contains(id) else { return }
        working.insert(id)
        defer { working.remove(id) }
        do {
            try await model.core.controlAgent(agentId: id, action: action)
            known[id] = switch action {
            case .pause: .paused
            case .resume: .active
            case .revoke: .revoked
            }
            action == .revoke ? Haptics.warning() : Haptics.commit()
            model.refresh()
        } catch {
            Haptics.dismiss()
            model.show(.init(kind: .error, text: String(localized: "Couldn’t reach Zoen. Check your connection and try again.")))
        }
    }
}

extension AgentStatus {
    var label: String {
        switch self {
        case .active: String(localized: "Active")
        case .paused: String(localized: "Paused")
        case .revoked: String(localized: "Turned off")
        }
    }
    var color: Color {
        switch self {
        case .active: Palette.success
        case .paused: Palette.amber
        case .revoked: Palette.textTertiary
        }
    }
}

/// The small "Ativo / Pausado" pill. Its color and word change together, so the state
/// never rests on motion or color alone.
struct AgentStatusPill: View {
    let status: AgentStatus
    var body: some View {
        HStack(spacing: 5) {
            Circle().fill(status.color).frame(width: 7, height: 7)
            Text(status.label).font(.caption.weight(.semibold))
                .contentTransition(.opacity)
        }
        .foregroundStyle(status == .revoked ? Palette.textTertiary : Palette.textPrimary)
        .padding(.horizontal, 9).padding(.vertical, 4)
        .background(status.color.opacity(0.14), in: .capsule)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("agentStatus")
    }
}

/// "N pedidos esperando você": your agent asked something in your chat with it.
struct AgentPendingBadge: View {
    let count: UInt32
    var body: some View {
        Text(count == 1 ? String(localized: "1 waiting for you") : String(localized: "\(Int(count)) waiting for you"))
            .font(.caption.weight(.semibold))
            .foregroundStyle(Palette.amber)
            .padding(.horizontal, 9).padding(.vertical, 4)
            .background(Palette.amber.opacity(0.14), in: .capsule)
            .accessibilityIdentifier("agentPending")
    }
}

/// The profile's top actions, like a person's: talk, share the link, pause or resume.
struct AgentCitizenActions: View {
    @Environment(AppModel.self) private var model
    let profile: AgentProfile
    @State private var invite: InviteDto?

    private var status: AgentStatus? { model.agentStatuses.known[profile.id] }

    var body: some View {
        VStack(spacing: 12) {
            if profile.persona.isMine {
                HStack(spacing: 8) {
                    if let status { AgentStatusPill(status: status) }
                    if profile.ownerNotesPending > 0 { AgentPendingBadge(count: profile.ownerNotesPending) }
                }
                .animation(.easeOut(duration: 0.2), value: status)
            }
            HStack(spacing: 10) {
                action(String(localized: "Talk"), symbol: "bubble.left.fill", id: "agent.chat") { openChat() }
                if profile.persona.isMine && status != .revoked {
                    if let link = invite?.link, let url = URL(string: link) {
                        ShareLink(item: url) { tile(String(localized: "Share link"), symbol: "square.and.arrow.up") }
                            .buttonStyle(PressScaleStyle())
                            .simultaneousGesture(TapGesture().onEnded { Haptics.tap() })
                            .accessibilityIdentifier("agent.share")
                    } else {
                        tile(String(localized: "Share link"), symbol: "square.and.arrow.up").opacity(0.45)
                            .accessibilityIdentifier("agent.share")
                    }
                    let paused = status == .paused
                    action(paused ? String(localized: "Resume") : String(localized: "Pause"),
                           symbol: paused ? "play.fill" : "pause.fill", id: "agent.pause") {
                        Task { await model.agentStatuses.control(profile.id, paused ? .resume : .pause, model: model) }
                    }
                    .disabled(status == nil || model.agentStatuses.working.contains(profile.id))
                    .opacity(status == nil ? 0.45 : 1)
                }
            }
        }
        .task(id: profile.id) {
            guard profile.persona.isMine else { return }
            await model.agentStatuses.load([profile.id], core: model.core)
            if invite == nil { invite = try? await model.core.createAgentInvite(agentId: profile.id) }
        }
    }

    /// The chat stacks on top of this profile, so Back comes right back here.
    private func openChat() {
        if let s = model.spaces.first(where: { $0.counterpart?.id == profile.id }) {
            model.push(.space(s.id))
        } else if let id = model.perform({ try model.core.startDirect(identityId: profile.id) }) {
            model.push(.space(id))
        }
    }

    private func action(_ title: String, symbol: String, id: String, run: @escaping () -> Void) -> some View {
        Button { Haptics.tap(); run() } label: { tile(title, symbol: symbol) }
            .buttonStyle(PressScaleStyle())
            .accessibilityIdentifier(id)
    }

    private func tile(_ title: String, symbol: String) -> some View {
        VStack(spacing: 6) {
            Image(systemName: symbol).font(.system(size: 18, weight: .semibold))
                .contentTransition(.symbolEffect(.replace))
            Text(title).font(.caption.weight(.semibold)).contentTransition(.opacity)
        }
        .foregroundStyle(Palette.action)
        .frame(maxWidth: .infinity)
        .padding(.vertical, 12)
        .glassEffect(.regular.interactive(), in: .rect(cornerRadius: 16, style: .continuous))
        .accessibilityElement(children: .combine)
    }
}

/// Revoking is final, so it asks first, in place, in plain words.
struct AgentDangerZone: View {
    @Environment(AppModel.self) private var model
    let profile: AgentProfile

    var body: some View {
        if profile.persona.isMine, let status = model.agentStatuses.known[profile.id], status != .revoked {
            VStack(alignment: .leading, spacing: 8) {
                ConfirmInPlaceButton(title: String(localized: "Turn off for good"), systemImage: "power",
                                     confirmTitle: String(localized: "Turn off \(profile.persona.name)? It stops answering everyone, and its link stops working"),
                                     doneTitle: String(localized: "Turned off"), identifier: "agent.revoke") {
                    Task { await model.agentStatuses.control(profile.id, .revoke, model: model) }
                }
                Text("This can’t be undone. To pause for a while, use Pause.")
                    .font(.caption).foregroundStyle(Palette.textTertiary)
            }
            .glassCard()
        }
    }
}
