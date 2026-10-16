import SwiftUI

struct LivingTierInviteView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let invite: LivingTierInvite
    @State private var requested = false
    @State private var failure: String?

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Label("Invitation", systemImage: "person.crop.circle.badge.checkmark").font(.title2.bold())
                    Text("Request this membership. An admin reviews your request before you receive private content.")
                    if requested { Label("Request sent · Awaiting approval", systemImage: "clock").foregroundStyle(.secondary) }
                    else {
                        Button("Request access", systemImage: "person.badge.plus") {
                            do {
                                let data = try JSONSerialization.data(withJSONObject: ["action": "tier_redeem", "token": invite.token])
                                _ = try model.core.livingSpaceCommand(spaceId: invite.space, commandJson: String(decoding: data, as: UTF8.self))
                                requested = true
                                model.refresh()
                            } catch { failure = error.localizedDescription }
                        }
                    }
                    if let failure { Text(failure).foregroundStyle(.red) }
                }
            }
            .navigationTitle("Invitation")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
        }
    }
}
