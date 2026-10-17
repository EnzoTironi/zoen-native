import SwiftUI
import RodaCore

/// Tiers are encrypted partitions, not cosmetic locks on already-public content.
struct LivingTiersView: View {
    @Environment(AppModel.self) private var model
    let spaceId: String
    @State private var home: LivingSpaceProjection?
    @State private var name = ""
    @State private var groupTitle = ""
    @State private var pageTitle = ""
    @State private var failure: String?
    @State private var inviteURL: URL?
    @State private var inviteExpiry = Date().addingTimeInterval(7 * 86400)

    private var admin: Bool { home?.canManage == true }
    private var tiers: [LivingTier] { home?.tiers ?? [] }
    private var administratorIds: Set<String> {
        Set(((try? model.core.memberRoles(spaceId: spaceId)) ?? [])
            .filter { $0.role == "owner" || $0.role == "admin" }.map(\.identityId))
    }

    var body: some View {
        Form {
            Section {
                Text("Memberships unlock private conversations, pages and events. Removing access cannot erase content someone already received.")
                    .font(.subheadline).foregroundStyle(.secondary)
            }
            if admin {
                Section("Create membership") {
                    TextField("Membership name", text: $name)
                    Button("Create membership", systemImage: "plus") { command(["action": "tier_create", "name": name]) }
                        .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
            ForEach(tiers) { tier in
                Section {
                    Text(tier.name).font(.caption.weight(.medium)).padding(.horizontal, 8).padding(.vertical, 4)
                        .background(.quaternary, in: Capsule())
                    if tier.members.contains(model.me?.id ?? "") { Label("Access granted", systemImage: "checkmark.circle").foregroundStyle(.secondary) }
                    else { Label("Not a member", systemImage: "lock").foregroundStyle(.secondary) }
                    ForEach(tier.groups, id: \.self) { id in
                        NavigationLink(model.space(id)?.title ?? String(localized: "Members conversation")) { LivingSpaceConversation(spaceId: id) }
                    }
                    ForEach(tier.resources) { resource in
                        let title = model.core.items().first { $0.id == resource.id }?.title ?? resource.kind
                        if resource.kind == "event" {
                            NavigationLink(title) { LivingEventView(itemId: resource.id) }
                        } else {
                            Button(title) { model.push(.item(resource.id)) }
                        }
                    }
                    if admin { management(tier) }
                } header: { Text(tier.name) }
            }
            if tiers.isEmpty { Section { Text("No memberships yet.").foregroundStyle(.secondary) } }
            if let inviteURL {
                Section("Invitation") {
                    ShareLink("Share invitation", item: inviteURL)
                    Text("An admin reviews each request before access is granted.")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            if admin && !(home?.invitations ?? []).isEmpty {
                Section("Invitations") {
                    ForEach(home?.invitations ?? []) { invitation in
                        VStack(alignment: .leading, spacing: 8) {
                            Text(tiers.first { $0.id == invitation.tierId }?.name ?? String(localized: "Membership")).font(.headline)
                            LabeledContent("Expires") { Text(invitation.expiry, format: .dateTime.month().day().hour().minute()) }
                            if invitation.revoked { Label("Revoked", systemImage: "xmark.circle").foregroundStyle(.secondary) }
                            else {
                                Button("Revoke invitation", role: .destructive) {
                                    command(["action": "tier_invite_revoke", "invite_id": invitation.id])
                                    if failure == nil { inviteURL = nil }
                                }.accessibilityIdentifier("living-space.invitation.revoke")
                            }
                        }.padding(.vertical, 6)
                    }
                }
            }
            if let failure { Section { Text(failure).foregroundStyle(.red) } }
        }
        .navigationTitle(admin ? "Manage memberships" : "Your memberships")
        .task(id: model.revision) { reload() }
        .accessibilityIdentifier("living-space.tiers")
    }

    @ViewBuilder private func management(_ tier: LivingTier) -> some View {
        Menu("Assign member", systemImage: "person.badge.plus") {
            ForEach(model.space(spaceId)?.members.filter { $0.kind == .person } ?? [], id: \.id) { person in
                Button(person.name) { command(["action": "tier_assign", "tier_id": tier.id, "identity_id": person.id]) }
            }
        }
        ForEach(tier.members, id: \.self) { identity in
            HStack {
                Text(model.core.personas().first { $0.id == identity }?.name ?? String(localized: "Community member")).font(.subheadline)
                Spacer()
                if administratorIds.contains(identity) {
                    Text("Administrator access").font(.caption).foregroundStyle(.secondary)
                } else {
                    Button("Revoke", role: .destructive) { command(["action": "tier_revoke", "tier_id": tier.id, "identity_id": identity]) }
                        .buttonStyle(.borderless)
                }
            }
        }
        DatePicker("Invitation expires", selection: $inviteExpiry, in: Date()...)
        Button("Create invitation", systemImage: "link") {
            command(["action": "tier_invite", "tier_id": tier.id, "expires_at_ms": Int64(inviteExpiry.timeIntervalSince1970 * 1000)])
        }
        TextField("Conversation name", text: $groupTitle)
        Button("Create encrypted conversation", systemImage: "lock.bubble") {
            command(["action": "tier_group_create", "tier_id": tier.id, "title": groupTitle])
        }.disabled(groupTitle.isEmpty)
        TextField("Page title", text: $pageTitle)
        Button("Create members page", systemImage: "doc.badge.plus") {
            if let partition = tier.space, let item = model.perform({ try model.core.pageCreate(spaceId: partition, title: pageTitle) }) {
                command(["action": "tier_bind", "tier_id": tier.id, "item_id": item.id, "kind": "page"])
            }
        }.disabled(pageTitle.isEmpty)
        Menu("Add private content", systemImage: "lock.open") {
            ForEach(model.core.items().filter { $0.spaceId == tier.space && ($0.kindId == "page" || $0.app?.appId == "schedule") }, id: \.id) { item in
                Button(item.title) {
                    command(["action": "tier_bind", "tier_id": tier.id, "item_id": item.id, "kind": item.kindId == "page" ? "page" : "event"])
                }
            }
        }
    }

    private func reload() {
        do {
            let json = try model.core.livingSpaceCommand(spaceId: spaceId, commandJson: "{\"action\":\"home\"}")
            home = try LivingSpaceProjection.decode(json)
            failure = nil
        } catch { failure = error.localizedDescription }
    }

    private func command(_ value: [String: Any]) {
        do {
            let data = try JSONSerialization.data(withJSONObject: value)
            let response = try model.core.livingSpaceCommand(spaceId: spaceId, commandJson: String(decoding: data, as: UTF8.self))
            if let result = try? JSONSerialization.jsonObject(with: Data(response.utf8)) as? [String: Any],
               let raw = result["invite_url"] as? String {
                inviteURL = URL(string: raw)
            }
            model.refresh()
            reload()
        } catch { failure = error.localizedDescription }
    }
}
