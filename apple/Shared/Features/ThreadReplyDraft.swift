import Foundation

/// A failed send must preserve both the reply and its broadcast choice for retry.
struct ThreadReplyDraft {
    var text = ""
    var alsoInChat = false

    @discardableResult
    mutating func submit(_ send: (String, Bool) -> Bool) -> Bool {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, send(trimmed, alsoInChat) else { return false }
        text = ""
        alsoInChat = false
        return true
    }
}
