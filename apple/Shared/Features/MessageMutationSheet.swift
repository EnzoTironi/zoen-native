import SwiftUI
import RodaCore

/// Reusable edit/delete confirmation. Only success closes the sheet; failures stay retryable.
struct MessageMutationSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let spaceId: String
    let messageId: String
    let deleting: Bool
    @State private var draft: MessageMutationDraft

    init(spaceId: String, messageId: String, text: String = "", deleting: Bool = false) {
        self.spaceId = spaceId
        self.messageId = messageId
        self.deleting = deleting
        _draft = State(initialValue: MessageMutationDraft(text: text))
    }

    var body: some View {
        NavigationStack {
            Form {
                if deleting {
                    Text("A mensagem será excluída para todos. Essa ação não pode ser desfeita.")
                } else {
                    TextEditor(text: $draft.text)
                        .frame(minHeight: 140)
                        .accessibilityLabel("Mensagem")
                        .accessibilityIdentifier("message-edit-field")
                }
                if let error = draft.error {
                    Section {
                        Text(deleting ? "Não foi possível excluir a mensagem. Tente novamente." : "Não foi possível salvar. Seu texto foi mantido. Tente novamente.")
                        Text(error).font(.caption).foregroundStyle(.secondary)
                    }
                    .foregroundStyle(.red)
                    .accessibilityIdentifier("message-mutation-error")
                }
            }
            .navigationTitle(deleting ? "Excluir para todos?" : "Editar mensagem")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancelar") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(deleting ? "Excluir" : "Salvar", role: deleting ? .destructive : nil) { submit() }
                        .disabled(!deleting && draft.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                        .accessibilityIdentifier("message-mutation-submit")
                }
            }
        }
        .presentationDetents([.medium, .large])
        .interactiveDismissDisabled(draft.error != nil)
    }

    private func submit() {
        draft.submit { text in
            do {
                if deleting {
                    try model.core.deleteMessage(spaceId: spaceId, messageId: messageId)
                } else {
                    try model.core.editMessage(spaceId: spaceId, messageId: messageId, text: text)
                }
            } catch let error as CoreError {
                throw NSError(domain: "MessageMutation", code: 1, userInfo: [NSLocalizedDescriptionKey: error.message])
            }
        }
        model.refresh()
        if draft.isComplete { dismiss() }
    }
}
