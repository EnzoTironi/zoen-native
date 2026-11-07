import Foundation

@main enum AssistantContextTests {
  static func main() {
    typealias Snapshot = ZoenAssistantContextSnapshot
    let original = Snapshot.Message(id: "one", authorId: "alice", authorName: "Alice", text: "The meeting is at noon")
    let receipt = Snapshot(title: "Plans", messages: [original])
    precondition(receipt.isCurrent(title: "Plans", messages: ["one": original]))
    let unrelated = Snapshot.Message(id: "two", authorId: "bob", authorName: "Bob", text: "A new unrelated message")
    precondition(receipt.isCurrent(title: "Plans", messages: ["one": original, "two": unrelated]))
    precondition(!receipt.isCurrent(title: "Plans", messages: [:]), "Deleted or inaccessible source cannot remain in model history")
    precondition(!receipt.isCurrent(title: "Renamed", messages: ["one": original]))
    for changed in [
      Snapshot.Message(id: "one", authorId: "alice", authorName: "Alice", text: "The meeting is at three"),
      Snapshot.Message(id: "one", authorId: "other", authorName: "Alice", text: original.text),
      Snapshot.Message(id: "one", authorId: "alice", authorName: "Changed", text: original.text)
    ] { precondition(!receipt.isCurrent(title: "Plans", messages: ["one": changed])) }
    let modelMessages = receipt.value["messages"] as? [[String: String]]
    precondition(modelMessages == [["author": "Alice", "text": original.text]], "Internal source identifiers do not enter the model prompt")
    print("PASS native assistant context: edits, deletions, attribution, titles, unrelated arrivals and private references")
  }
}
