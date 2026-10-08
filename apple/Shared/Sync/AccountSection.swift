import SwiftUI
import RodaCore

/// "You" → your account on the relay: who you are there, whether you're connected,
/// fixing a taken @, and signing this device out.
struct AccountSection: View {
    @Environment(AppModel.self) private var model
    @State private var editing = false
    @State private var confirmErase = false

    var body: some View {
        let sync = model.sync
        if let acct = sync.account {
            Section {
                if sync.connection.error == "handle_taken" {
                    Button { editing = true } label: {
                        Label {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("@\(acct.handle) is taken").font(.subheadline.weight(.semibold))
                                Text("Pick another @ so people can find you. Your chats wait on this device.")
                                    .font(.caption).foregroundStyle(Palette.textSecondary)
                            }
                        } icon: {
                            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                        }
                    }
                    .accessibilityIdentifier("handleTaken")
                }
                if sync.keyMissing {
                    Label("This device lost its key. Erase it and sign up again to keep chatting.", systemImage: "key.slash")
                        .font(.subheadline).foregroundStyle(Palette.danger)
                }
                Button { editing = true } label: {
                    LabeledContent {
                        Text("@\(acct.handle)")
                    } label: {
                        Label(acct.name.isEmpty ? String(localized: "Profile") : acct.name, systemImage: "person.crop.circle")
                            .foregroundStyle(Palette.textPrimary)
                    }
                }
                .accessibilityIdentifier("editProfile")
                LabeledContent {
                    Text(stateLabel(sync.connection)).foregroundStyle(stateColor(sync.connection))
                } label: {
                    Label("Connection", systemImage: "point.3.connected.trianglepath.dotted")
                }
                LabeledContent("Device") {
                    Text(verbatim: "\(acct.deviceId.prefix(8))…").font(.caption.monospaced())
                }
                Button("Erase this device…", role: .destructive) { confirmErase = true }
                    .accessibilityIdentifier("eraseDevice")
            } header: {
                Text("Account")
            } footer: {
                Text("Your chats stay in sync across your devices. Erasing removes them from this device only.")
            }
            .sheet(isPresented: $editing) { ProfileEditor(account: acct) }
            .confirmationDialog("Erase this device?", isPresented: $confirmErase, titleVisibility: .visible) {
                Button("Erase", role: .destructive) {
                    do { try sync.eraseDevice() } catch {
                        model.show(.init(kind: .error, text: (error as? CoreError)?.message ?? error.localizedDescription))
                    }
                }
            } message: {
                Text("Chats and keys on this device are deleted. Without another signed-in device, this account can't be recovered yet.")
            }
        }
    }

    private func stateLabel(_ c: ConnectionDto) -> String {
        switch c.state {
        case "online": return c.synced ? String(localized: "Connected") : String(localized: "Syncing…")
        case "connecting": return String(localized: "Connecting…")
        default:
            if c.pending > 0 { return String(localized: "Offline · \(c.pending) queued") }
            return String(localized: "Offline")
        }
    }

    private func stateColor(_ c: ConnectionDto) -> Color {
        switch c.state {
        case "online": return Palette.success
        case "connecting": return .orange
        default: return Palette.textSecondary
        }
    }

}

/// Name and @ after sign-up. A changed @ is re-registered with the relay right away.
private struct ProfileEditor: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let account: AccountDto
    @State private var name = ""
    @State private var handle = ""
    @State private var failure: String?

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    AvatarPhotoPicker(personaId: account.identityId, pending: .constant(nil), size: 88,
                                      initials: initials, tintHex: "#6B8F71")
                        .frame(maxWidth: .infinity)
                        .listRowBackground(Color.clear)
                    TextField("Your name", text: $name).textContentType(.name)
                    HStack(spacing: 2) {
                        Text(verbatim: "@").foregroundStyle(Palette.textSecondary)
                        TextField("username", text: Binding(get: { handle }, set: { handle = ProfileFields.clean($0) }))
                            .textContentType(.username)
                            .autocorrectionDisabled()
                            #if os(iOS)
                            .textInputAutocapitalization(.never)
                            #endif
                    }
                } footer: {
                    if let failure { Text(failure).foregroundStyle(Palette.danger) }
                }
            }
            .navigationTitle("Profile")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save", action: save)
                        .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty || handle.count < 3)
                }
            }
        }
        .onAppear { name = account.name; handle = account.handle }
        #if os(macOS)
        .frame(minWidth: 360, minHeight: 220)
        #endif
    }

    private var initials: String {
        let parts = name.split { $0.isWhitespace }.prefix(2)
        let chars = parts.compactMap({ $0.first }).map(String.init)
        return chars.isEmpty ? "?" : chars.joined().uppercased()
    }

    private func save() {
        do {
            try model.sync.updateProfile(name: name, handle: handle)
            dismiss()
        } catch {
            failure = (error as? CoreError)?.message ?? error.localizedDescription
        }
    }
}
