import Foundation

/// Volatile observations of the text supplied to one local model turn, never a
/// read permit. New unrelated messages do not invalidate these observations.
struct ZoenAssistantContextSnapshot: Sendable {
  struct Message: Equatable, Sendable {
    let id: String
    let authorId: String
    let authorName: String
    let text: String
  }
  let title: String
  let messages: [Message]

  var value: [String: Any] {
    ["title": title, "messages": messages.map { ["author": $0.authorName, "text": $0.text] }]
  }

  func isCurrent(title: String, messages current: [String: Message]) -> Bool {
    self.title == title && messages.allSatisfy { current[$0.id] == $0 }
  }
}
