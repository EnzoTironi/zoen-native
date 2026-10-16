import Foundation

/// A failed mutation must not consume the user's draft or dismiss its editor.
struct MessageMutationDraft {
    var text: String
    private(set) var error: String?
    private(set) var isComplete = false

    mutating func submit(_ operation: (String) throws -> Void) {
        do {
            try operation(text)
            error = nil
            isComplete = true
        } catch {
            self.error = error.localizedDescription
            isComplete = false
        }
    }
}
