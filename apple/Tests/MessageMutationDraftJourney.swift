import Foundation

@main
struct MessageMutationDraftJourney {
    static func main() {
        var draft = MessageMutationDraft(text: "texto alterado com ação")
        var persisted = "texto antigo"
        let failure = NSError(domain: "test", code: 1, userInfo: [NSLocalizedDescriptionKey: "Sem permissão"])
        draft.submit { _ in throw failure }
        precondition(draft.text == "texto alterado com ação", "Failure consumed the edit draft")
        precondition(!draft.isComplete, "Failure dismissed the editor")
        precondition(draft.error == "Sem permissão", "Failure was hidden")
        precondition(persisted == "texto antigo")
        draft.text += " revisado"
        draft.submit { persisted = $0 }
        precondition(persisted == "texto alterado com ação revisado")
        precondition(draft.isComplete && draft.error == nil, "Successful retry did not dismiss/clear error")

        var deletion = MessageMutationDraft(text: "")
        var deleted = false
        deletion.submit { _ in throw failure }
        precondition(!deletion.isComplete && deletion.error != nil && !deleted)
        deletion.submit { _ in deleted = true }
        precondition(deletion.isComplete && deletion.error == nil && deleted)
        print("PASS: failed edits preserve draft/error; retry commits latest draft; deletion closes only after success")
    }
}
