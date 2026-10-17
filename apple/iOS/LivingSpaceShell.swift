import SwiftUI
import RodaCore

/// The experiment owns its navigation without changing the shipped default shell.
struct LivingSpaceShell: View {
    @Environment(AppModel.self) private var model
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var tab = 0
    @State private var pending = 0
    @State private var tierInvite: LivingTierInvite?

    var body: some View {
        Group {
            if sizeClass == .regular {
                NavigationSplitView {
                    List {
                        Button("Spaces", systemImage: "square.stack.3d.up") { tab = 0 }
                        Button { tab = 1 } label: {
                            Label(pending > 0 ? "Notifications (\(pending))" : "Notifications", systemImage: "bell")
                        }
                        if tab == 0 { spaceRows }
                    }.navigationTitle("Zoen")
                } detail: {
                    if tab == 1 { NavigationStack { notifications } }
                    else { spacesStack(showsList: false) }
                }
            } else {
                TabView(selection: $tab) {
                    Tab("Spaces", systemImage: "square.stack.3d.up", value: 0) { spacesStack(showsList: true) }
                    Tab("Notifications", systemImage: "bell", value: 1) {
                        NavigationStack { notifications }
                    }.badge(pending)
                }
            }
        }
        .onOpenURL { url in
            if let invite = LivingTierInvite(url: url) { tierInvite = invite }
            #if DEBUG
            if url.scheme == "zoen", url.host == "living",
               let screen = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?.first(where: { $0.name == "screen" })?.value {
                tab = screen == "notifications" ? 1 : 0
                NotificationCenter.default.post(name: Notification.Name("livingCaptureRoute"), object: screen)
            }
            #endif
        }
        .sheet(item: $tierInvite) { invite in
            LivingTierInviteView(invite: invite)
        }
        .overlay {
            if let error = model.bootError {
                ContentUnavailableView("Couldn't open this account", systemImage: "exclamationmark.triangle", description: Text(error))
                    .frame(maxWidth: .infinity, maxHeight: .infinity).background(.background)
                    .accessibilityIdentifier("living-space.boot-error")
            } else if model.sync.needsAccount { OnboardingFlow {} }
        }
        .task(id: model.revision) {
            pending = model.spaces.filter { $0.kind == .community }.reduce(0) { count, space in
                guard let json = try? model.core.livingSpaceCommand(spaceId: space.id, commandJson: "{\"action\":\"home\"}"),
                      let home = try? LivingSpaceProjection.decode(json), home.canManage else { return count }
                return count + home.pendingRequests.count + (home.tierClaims?.count ?? 0)
            }
        }
        .task {
            #if DEBUG
            if (SyncModel.mode == .demo && UserDefaults.standard.bool(forKey: "RodaLivingSpaceFixture")) || UserDefaults.standard.bool(forKey: "RodaLivingAccountFixture") {
                let title = "Living Space"
                if let id = model.spaces.first(where: { $0.kind == .community && $0.title == title })?.id {
                    model.go(.space(id))
                } else if let id = try? model.core.createCommunity(title: title) {
                    model.refresh()
                    model.go(.space(id))
                }
            }
            if UserDefaults.standard.string(forKey: "RodaLivingSpaceScreen") == "notifications" { tab = 1 }
            #endif
        }
    }

    private var notifications: some View {
        LivingSpaceNotifications(onCount: { pending = $0 })
    }

    private func spacesStack(showsList: Bool) -> some View {
        NavigationStack(path: Binding(get: { model.path(.conversations) }, set: { model.setPath(.conversations, $0) })) {
            Group {
                if showsList { List { spaceRows }.navigationTitle("Spaces") }
                else { ContentUnavailableView("Choose a Space", systemImage: "square.stack.3d.up") }
            }
            .navigationDestination(for: Route.self) { route in RouteDestination(route: route) }
        }
    }

    @ViewBuilder private var spaceRows: some View {
        ForEach(model.spaces.filter { $0.kind == .community }, id: \.id) { space in
            Button { model.go(.space(space.id)) } label: {
                Label(space.title, systemImage: "square.stack.3d.up")
            }
        }
    }
}

private struct LivingSpaceNotifications: View {
    @Environment(AppModel.self) private var model
    let onCount: (Int) -> Void
    @State private var homes: [String: LivingSpaceProjection] = [:]
    @State private var failure: String?
    @State private var loaded = false

    var body: some View {
        List {
            if !loaded && failure == nil { ProgressView("Loading notifications") }
            if loaded && failure == nil && homes.values.allSatisfy({ $0.pendingRequests.isEmpty && ($0.tierClaims ?? []).isEmpty }) {
                ContentUnavailableView("You're all caught up", systemImage: "bell",
                    description: Text("Join requests for Spaces you manage appear here."))
            }
            ForEach(model.spaces.filter { homes[$0.id]?.canManage == true }, id: \.id) { space in
                if let home = homes[space.id], !home.pendingRequests.isEmpty {
                    Section(space.title) {
                        ForEach(home.pendingRequests, id: \.self) { identity in
                            VStack(alignment: .leading, spacing: 12) {
                                Label("Join request", systemImage: "person.badge.clock").font(.headline)
                                Text(model.core.personas().first { $0.id == identity }?.name ?? String(localized: "Community member")).font(.subheadline)
                                HStack {
                                    Button("Approve", systemImage: "checkmark") { respond("approve_join", identity: identity, space: space.id) }
                                    Button("Decline", systemImage: "xmark", role: .destructive) { respond("reject_join", identity: identity, space: space.id) }
                                }.buttonStyle(.bordered)
                            }.padding(.vertical, 8)
                        }
                    }
                }
            }
            ForEach(model.spaces.filter { homes[$0.id]?.canManage == true }, id: \.id) { space in
                if let home = homes[space.id], !(home.tierClaims ?? []).isEmpty {
                    Section("Membership requests · \(space.title)") {
                        ForEach(Array((home.tierClaims ?? []).enumerated()), id: \.offset) { _, claim in
                            VStack(alignment: .leading, spacing: 12) {
                                Text(home.tiers?.first { $0.id == claim.tierId }?.name ?? String(localized: "Membership request")).font(.headline)
                                Text(model.core.personas().first { $0.id == claim.identityId }?.name ?? String(localized: "Community member")).font(.subheadline)
                                Button("Approve membership", systemImage: "checkmark") {
                                    approveTier(claim, space: space.id)
                                }.buttonStyle(.bordered)
                            }.padding(.vertical, 8)
                        }
                    }
                }
            }
            if let failure { Text(failure).foregroundStyle(.red) }
        }
        .navigationTitle("Notifications")
        .task(id: model.revision) { reload() }
    }

    private func reload() {
        do {
            try model.core.setLivingSpaceEnabled(enabled: true)
            var next: [String: LivingSpaceProjection] = [:]
            for space in model.spaces where space.kind == .community {
                let json = try model.core.livingSpaceCommand(spaceId: space.id, commandJson: "{\"action\":\"home\"}")
                next[space.id] = try LivingSpaceProjection.decode(json)
            }
            homes = next
            onCount(next.values.filter(\.canManage).reduce(0) { $0 + $1.pendingRequests.count + ($1.tierClaims?.count ?? 0) })
            failure = nil
            loaded = true
        } catch { failure = error.localizedDescription; loaded = false }
    }

    private func approveTier(_ claim: LivingTierClaim, space: String) {
        do {
            let data = try JSONSerialization.data(withJSONObject: ["action": "tier_claim_approve", "identity_id": claim.identityId, "invite_id": claim.inviteId])
            _ = try model.core.livingSpaceCommand(spaceId: space, commandJson: String(decoding: data, as: UTF8.self))
            model.refresh()
            reload()
        } catch { failure = error.localizedDescription }
    }

    private func respond(_ action: String, identity: String, space: String) {
        do {
            let data = try JSONSerialization.data(withJSONObject: ["action": action, "identity_id": identity])
            _ = try model.core.livingSpaceCommand(spaceId: space, commandJson: String(decoding: data, as: UTF8.self))
            model.refresh()
            reload()
        } catch { failure = error.localizedDescription }
    }
}
