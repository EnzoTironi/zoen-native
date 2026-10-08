import SwiftUI
import RodaCore

/// Participantes (poster #016): Pessoas e Agentes separados, permissões por agente
/// e a configuração da conversa (convites, privacidade, registro).
struct ParticipantsView: View {
    @Environment(AppModel.self) private var model
    let spaceId: String
    @State private var invite: String?
    /// A relay invite for a shared chat (code + link).
    @State private var relayInvite: InviteDto?
    @State private var inviting = false
    @State private var backgroundPicker = false
    @State private var artPicker: AvatarArtPicker.Target?

    private var space: SpaceSummary? { model.space(spaceId) }

    var body: some View {
        List {
            if let space {
                let people = space.members.filter { $0.kind == .person }
                let agents = space.members.filter { $0.kind == .agent }

                Section {
                    ForEach(people) { m in
                        HStack(spacing: 12) {
                            Avatar(persona: m, size: 40)
                                .profileLink(m, enabled: !m.isMe)
                            VStack(alignment: .leading, spacing: 1) {
                                Text(m.isMe ? String(localized: "\(m.name) (you)") : m.name).font(.body.weight(.medium))
                                    .profileLink(m, enabled: !m.isMe)
                                Text("@\(m.handle)").font(.caption).foregroundStyle(Palette.textSecondary)
                            }
                        }
                        .profileLink(m, enabled: !m.isMe)
                    }
                } header: {
                    HStack {
                        Text("People")
                        Spacer()
                        inviteButton
                    }
                }

                Section("Agents") {
                    ForEach(agents) { a in
                        let level = model.agentProfile(a.id)?.spaces.first { $0.spaceId == spaceId }?.level
                        HStack(spacing: 12) {
                            // Same Avatar path as chat bubbles (paper + AvatarV1 / mascot).
                            Avatar(persona: a, size: 44)
                                .profileLink(a)
                            VStack(alignment: .leading, spacing: 1) {
                                Text(a.name).font(.body.weight(.medium))
                                    .profileLink(a)
                                Text("\(a.ownerName ?? "")’s · \(level?.label ?? "—")").font(.caption).foregroundStyle(Palette.textSecondary)
                            }
                            Spacer()
                            NavigationLink(value: Route.permissions(agent: a.id, space: spaceId)) {
                                Text("Permissions").font(.subheadline.weight(.medium))
                            }
                            .buttonStyle(.glass)
                            .fixedSize()
                        }
                    }
                }

                if space.counterpart == nil {
                    Section {
                        Button {
                            artPicker = .group(id: space.id, title: space.title)
                        } label: {
                            LabeledContent {
                                GroupAvatar(space: space, size: 36)
                            } label: {
                                Label(String(localized: "Choose drawing"), systemImage: "paintbrush.pointed")
                            }
                        }
                        .foregroundStyle(Palette.textPrimary)
                    }
                }

                Section {
                    Button { backgroundPicker = true } label: {
                        let bgState = model.backgroundState(spaceId)
                        let bg = bgState.background
                        LabeledContent {
                            ChatBackdropView(background: bg, spaceId: spaceId)
                                .frame(width: 26, height: 36)
                                .clipShape(.rect(cornerRadius: 6, style: .continuous))
                                .overlay(RoundedRectangle(cornerRadius: 6, style: .continuous).stroke(Palette.textPrimary.opacity(0.12)))
                        } label: {
                            Label { Text("Background") } icon: { ZoenIcon(.sun, size: 18) }
                        }
                        .foregroundStyle(Palette.textPrimary)
                    }
                    Label {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("History for agents")
                            Text("Since each agent joined").font(.caption).foregroundStyle(Palette.textSecondary)
                        }
                    } icon: { ZoenIcon(.list, size: 18) }
                    Label {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Who can trigger it")
                            Text("Members of this chat, with an @mention").font(.caption).foregroundStyle(Palette.textSecondary)
                        }
                    } icon: { ZoenIcon(.agents, size: 18) }
                    // Integrity: only surface a plain warning when something is wrong.
                    if !model.core.verifyLog(spaceId: spaceId).valid {
                        Label {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("This chat’s history looks damaged")
                                Text("Ask Zoen or check another device before you rely on it.")
                                    .font(.caption).foregroundStyle(Palette.textSecondary)
                            }
                        } icon: {
                            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(Palette.amber)
                        }
                    }
                } header: {
                    Label { Text("Chat settings") } icon: { ZoenIcon(.settings, size: 14) }
                }

                if let relayInvite {
                    Section {
                        ShareLink(item: String(localized: "Join me on Zoen: \(relayInvite.link) (code \(relayInvite.code))")) {
                            Label { Text("Share invite") } icon: { ZoenIcon(.share, size: 18) }
                        }
                        LabeledContent("Code") {
                            Text(relayInvite.code).font(.body.monospaced().weight(.semibold)).textSelection(.enabled)
                        }
                    } header: { Text("Invite") } footer: {
                        Text("Anyone with this code can join for 7 days. They paste it in New chat → Join.")
                    }
                }

                if model.sync.isSynced(spaceId) && space.kind == .group {
                    Section {
                        Button(role: .destructive) {
                            if model.perform({ try model.core.leaveSpace(spaceId: spaceId) }) != nil { model.setPath(model.tab, []) }
                        } label: { Text("Leave group") }
                    }
                }

                if let invite {
                    Section {
                        ShareLink(item: invite) { Label { Text("Share invite") } icon: { ZoenIcon(.share, size: 18) } }
                        Text(invite).font(.caption.monospaced()).foregroundStyle(Palette.textSecondary)
                    } header: { Text("Invite") } footer: {
                        Text("Valid for 7 days. The invite web page isn’t ready yet — sharing the link is enough for now.")
                    }
                }

                Section {
                    Label { Text("Jam (voice, video and co-editing) arrives in phase 2") } icon: { HStack(spacing: 2) { ZoenIcon(.phone, size: 15); ZoenIcon(.video, size: 15) } }
                        .foregroundStyle(Palette.textTertiary)
                }
            }
        }
        .sheet(item: $artPicker) { target in
            AvatarArtPicker(target: target)
        }
        .sheet(isPresented: $backgroundPicker) {
            ChatBackgroundPicker(spaceId: spaceId,
                                 current: model.backgroundState(spaceId).background,
                                 isLocal: model.backgroundState(spaceId).isLocal)
        }
        .navigationTitle("Participants")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
    }

    private var inviteButton: some View {
        Button("Add") {
            if model.sync.isSynced(spaceId) {
                inviting = true
                Task {
                    defer { inviting = false }
                    do { relayInvite = try await model.core.createInvite(spaceId: spaceId) }
                    catch { model.show(.init(kind: .error, text: (error as? CoreError)?.message ?? error.localizedDescription)) }
                }
            } else {
                invite = model.perform { try model.core.inviteLink(spaceId: spaceId) }
            }
        }
        .font(.subheadline.weight(.semibold))
        .textCase(nil)
        .disabled(inviting || (model.sync.isSynced(spaceId) && space?.kind != .group))
    }
}
