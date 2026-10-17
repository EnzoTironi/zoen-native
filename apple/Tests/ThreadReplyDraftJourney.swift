import Foundation

@main
struct ThreadReplyDraftJourney {
    static func main() {
        var draft = ThreadReplyDraft(text: "  Minha resposta\n", alsoInChat: true)
        var attempts = 0
        var submittedText = ""
        var broadcast = false
        let failed = draft.submit { text, alsoInChat in
            attempts += 1
            submittedText = text
            broadcast = alsoInChat
            return false
        }
        precondition(!failed, "Failed core submission must report failure")
        precondition(draft.text == "  Minha resposta\n", "Failure must preserve the full draft")
        precondition(draft.alsoInChat, "Failure must preserve the broadcast choice")
        precondition(attempts == 1 && submittedText == "Minha resposta" && broadcast,
                     "Submission must pass trimmed text and the selected destination")
        draft.text += "revisada"
        let succeeded = draft.submit { text, alsoInChat in
            attempts += 1
            submittedText = text
            broadcast = alsoInChat
            return true
        }
        precondition(succeeded && submittedText == "Minha resposta\nrevisada" && broadcast,
                     "Retry must send the latest draft with its original broadcast choice")
        precondition(draft.text.isEmpty && !draft.alsoInChat, "Only success clears the reply and broadcast choice")
        var blank = ThreadReplyDraft(text: " \n ", alsoInChat: true)
        let blankResult = blank.submit { _, _ in attempts += 1; return true }
        precondition(!blankResult && attempts == 2, "Whitespace-only replies must not reach the core")
        precondition(blank.text == " \n " && blank.alsoInChat, "A rejected blank draft must not be consumed")
        var threadOnly = ThreadReplyDraft(text: "Só aqui")
        precondition(threadOnly.submit { _, alsoInChat in !alsoInChat }, "Default replies must remain thread-only")
        print("PASS: 9 thread reply draft regressions")
    }
}
