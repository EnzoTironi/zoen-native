import SwiftUI
import ZoenCore

/// The existing, signed-in device approves a code only after the person compares both screens.
struct DeviceConnectionSection: View {
    @Environment(AppModel.self) private var model
    let refreshed: () -> Void
    @State private var code = ""
    @State private var check: String?
    @State private var busy = false
    @State private var failed = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Conectar aparelho").font(.headline)
            Text("No outro aparelho, abra o Zoen e copie o código de conexão. Cole aqui e confira os seis números nas duas telas.")
            TextField("Código de conexão", text: $code).textFieldStyle(.roundedBorder)
                .disabled(busy)
            Button("Conferir código") {
                do { check = try model.core.linkCodeCheck(code: code.trimmingCharacters(in: .whitespacesAndNewlines)); failed = false }
                catch { failed = true }
            }.disabled(busy || code.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            if busy { ProgressView("Conectando aparelho…") }
            if failed { Text("Não foi possível conectar. Confira o código e tente de novo.").foregroundStyle(.red) }
        }
        .alert("Os números são iguais?", isPresented: Binding(get: { check != nil }, set: { if !$0 { check = nil } })) {
            Button("Cancelar", role: .cancel) { check = nil }
            Button("Conectar") {
                check = nil
                busy = true
                Task { @MainActor in
                    defer { busy = false }
                    do {
                        _ = try await model.core.linkDevice(code: code.trimmingCharacters(in: .whitespacesAndNewlines), vault: model.sync.vault)
                        code = ""
                        failed = false
                        refreshed()
                    } catch { failed = true }
                }
            }
        } message: { Text("Confira nas duas telas: \(check ?? ""). Só conecte se forem iguais.") }
    }
}
