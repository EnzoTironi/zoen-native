import SwiftUI
import ClientCore

// Kept only in view memory: no defaults, analytics, clipboard or error interpolation.
struct RecoveryKeySetup: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var scenePhase
    let done: (BackupStatusDto) -> Void
    @State private var key = ""
    @State private var confirmation = ""
    @State private var confirming = false
    @State private var busy = false
    @State private var error: String?

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text("Guarde esta chave fora deste aparelho. Ela abre suas conversas se você perder todos os aparelhos. O Zoen não pode recuperá-la por você.")
                    if !confirming {
                        if scenePhase == .active {
                            Text(verbatim: key).font(.body.monospaced()).privacySensitive()
                                .accessibilityIdentifier("recoveryKey")
                        }
                        Button("Já guardei a chave") { confirming = true }
                            .disabled(key.isEmpty)
                    } else {
                        Text("Digite a chave que você guardou para conferir antes de ativar a cópia.")
                        SecureField("Chave de 64 números", text: $confirmation)
                            .autocorrectionDisabled().disabled(busy)
                            .accessibilityIdentifier("recoveryConfirmation")
                        Button("Ver a chave novamente") { confirming = false; confirmation = "" }.disabled(busy)
                    }
                    if let error { Text(error).foregroundStyle(.red).accessibilityIdentifier("recoveryError") }
                }
                if confirming {
                    Button(busy ? "Enviando cópia…" : "Ativar cópia") { Task { await activate() } }
                        .disabled(busy || !RecoveryKeyInput.matches(key, confirmation))
                        .accessibilityIdentifier("recoveryActivate")
                }
                Text("Se o envio falhar, tente novamente com esta mesma chave. Se fechar esta tela, a chave não ficará guardada no aplicativo.")
            }
            .navigationTitle("Chave de recuperação")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Fechar") { dismiss() }.disabled(busy) } }
        }
        .interactiveDismissDisabled(busy)
        .task { if key.isEmpty { key = model.core.backupCreateRecoveryKey() } }
        .onDisappear { key = ""; confirmation = "" }
    }

    private func activate() async {
        guard !busy, RecoveryKeyInput.matches(key, confirmation) else { return }
        busy = true; error = nil
        defer { busy = false }
        do {
            let status = try await model.core.backupEnableRecoveryKey(recoveryKey: key, vault: model.sync.vault)
            done(status)
            key = ""; confirmation = ""
            dismiss()
        } catch {
            // Do not echo arbitrary server responses: they can include submitted values.
            self.error = "Não foi possível concluir a cópia. Sua chave continua nesta tela. Confira a conexão e tente novamente."
        }
    }
}

struct RecoveryKeyRestore: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let done: () -> Void
    @State private var relay = SyncModel.defaultRelayURL
    @State private var handle = ""
    @State private var key = ""
    @State private var busy = false
    @State private var error: String?

    var body: some View {
        NavigationStack {
            Form {
                Text("Use a chave de 64 números que você guardou ao ativar a cópia. Não é a senha do aparelho.")
                TextField("Endereço do servidor", text: $relay).autocorrectionDisabled().disabled(busy)
                TextField("Seu @", text: $handle).autocorrectionDisabled().disabled(busy)
                SecureField("Chave de 64 números", text: $key).autocorrectionDisabled().disabled(busy)
                    .accessibilityIdentifier("recoveryRestoreKey")
                if let error { Text(error).foregroundStyle(.red) }
                Button(busy ? "Recuperando…" : "Recuperar minha conta") { Task { await restore() } }
                    .disabled(busy || !RecoveryKeyInput.valid(key) || handle.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !RecoveryKeyInput.validRelay(relay))
                    .accessibilityIdentifier("recoveryRestore")
            }
            .navigationTitle("Recuperar minha conta")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancelar") { dismiss() }.disabled(busy) } }
        }
        .interactiveDismissDisabled(busy)
        .onDisappear { key = "" }
    }

    private func restore() async {
        guard !busy, RecoveryKeyInput.valid(key), RecoveryKeyInput.validRelay(relay) else { return }
        busy = true; error = nil
        defer { busy = false }
        do {
            _ = try await model.core.restoreBackupWithRecoveryKey(relayUrl: relay.trimmingCharacters(in: .whitespacesAndNewlines), handle: handle.trimmingCharacters(in: .whitespacesAndNewlines), recoveryKey: key, vault: model.sync.vault)
            model.sync.start(core: model.core)
            model.refresh()
            key = ""
            dismiss()
            done()
        } catch {
            self.error = "Não foi possível recuperar a conta. Confira seu @, a chave, o servidor e a conexão. Cópias com senha precisam de um aparelho já conectado."
        }
    }
}
