import Foundation

// Error port only. This tests the actual Foundation request decoder, not Core DTOs,
// journal persistence, engine authority, UIKit or generated bindings.
enum ZoenPageInputLease { enum Failure: Error { case invalidCapture, unsupported } }

@main struct PageInputOpenRequestTests {
  static func check(_ condition: Bool, _ message: String = "Native target assertion") {
    if !condition { fatalError(message) }
  }
  static func request(_ changes: [String: Any] = [:]) throws -> String {
    let base: [String: Any] = ["session": "native-session", "id": "input-id", "itemId": "page-item",
      "blockId": NSNull(), "resume": false, "revision": "7", "editContext": "observed-context", "mode": "markdown"]
    return String(decoding: try JSONSerialization.data(withJSONObject: base.merging(changes) { _, next in next }), as: UTF8.self)
  }
  static func decode(_ changes: [String: Any] = [:]) throws -> ZoenPageInputOpenRequest {
    try ZoenPageInputOpenRequest.decode(request(changes))
  }
  static func rejected(_ changes: [String: Any]) throws {
    do { _ = try decode(changes) }
    catch { return }
    fatalError("Invalid target shape was admitted")
  }
  static func main() throws {
    let document = try decode(["target": "document"])
    let legacyAppend = try decode()
    check(document != legacyAppend, "Whole-document and append are distinct targets, even though blockId is nil")
    check(try decode(["target": "append"]) == legacyAppend)
    let legacyBlock = try decode(["blockId": "existing-block"])
    check(try decode(["target": "block", "blockId": "existing-block"]) == legacyBlock)
    for invalid: [String: Any] in [
      ["target": NSNull()], ["target": "other"], ["target": 1],
      ["target": "document", "initialMarkdown": "forged JavaScript seed"],
      ["target": "document", "blockId": "existing-block"],
      ["target": "document", "mode": "rich"], ["target": "document", "mode": "lossless-text"],
      ["target": "append", "blockId": "existing-block"], ["target": "block"],
      ["target": "block", "blockId": ""], ["revision": "07"], ["unknown": true]
    ] { try rejected(invalid) }
    check(try decode(["target": "document", "resume": true]) != decode(["target": "append", "resume": true]))
    let capturedBlock = try ZoenPageInputOpenRequest.seedBlock("{\"Page\":{\"kind\":\"paragraph\",\"text\":\"Existing🪴\",\"spans\":[]}}")
    check(capturedBlock?["text"] as? String == "Existing🪴")
    check(try ZoenPageInputOpenRequest.seedBlock("null") == nil)
    print("PASS native Page target normalization, legacy compatibility, whole-document isolation and strict shapes")
  }
}
