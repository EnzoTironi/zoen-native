import Foundation

@MainActor private final class ConversationProbe: ZoenFilePreviewPresentation {
  var ended: (@MainActor (Error?) -> Void)?
  var dismiss: (@MainActor () -> Void)?
  func show(file: URL, title: String, ended: @escaping @MainActor (Error?) -> Void) throws { self.ended = ended }
  func dismiss(completion: @escaping @MainActor () -> Void) { dismiss = completion }
}

/// Host contract tests execute the actual FilePreview copy owner. The probe
/// controls dismissal timing only; no UIKit/render/Fabric proof is implied.
@main @MainActor struct ConversationGalleryLeaseTests {
  static func main() async throws {
    let root = URL.temporaryDirectory.appendingPathComponent("zoen-gallery-lease-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    func file() throws -> ZoenPreviewFile {
      let directory = root.appendingPathComponent(UUID().uuidString)
      try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
      let url = directory.appendingPathComponent("owned.bin")
      try Data("synthetic lease bytes".utf8).write(to: url)
      return .init(url: url, directory: directory)
    }
    func exists(_ file: ZoenPreviewFile) -> Bool { FileManager.default.fileExists(atPath: file.url.path) }
    let owner = ZoenFilePreview(), probe = ConversationProbe(), copies = ZoenFilePreview.ReadOnlyCopies()
    let first = try file(), second = try file(), third = try file(), fourth = try file()
    let ticket = owner.register(file: first, generation: 1)
    let work = Task { try await owner.open(id: ticket, title: "QA", edit: nil, readOnlyCopies: copies, initialKey: "a") { _ in probe } }
    while probe.ended == nil { await Task.yield() }
    let secondTicket=try owner.registerReadOnly(file:second,generation:1,owner:copies.registration)
    let thirdTicket=try owner.registerReadOnly(file:third,generation:1,owner:copies.registration)
    let secondURL=try copies.accept(secondTicket,key:"b"),thirdURL=try copies.accept(thirdTicket,key:"c")
    precondition(secondURL == second.url && thirdURL == third.url)
    precondition(exists(first) && exists(second) && exists(third))
    let overflow = try file()
    let overflowTicket=try owner.registerReadOnly(file:overflow,generation:1,owner:copies.registration)
    do { _ = try copies.accept(overflowTicket, key:"overflow"); preconditionFailure("Exceeded3-page window") } catch {}
    precondition(!exists(overflow))
    try copies.release(except: ["b", "c"])
    precondition(!exists(first) && exists(second) && exists(third))
    let fourthTicket=try owner.registerReadOnly(file:fourth,generation:1,owner:copies.registration)
    _ = try copies.accept(fourthTicket,key:"d")
    // Clear waits for the actual hierarchy dismissal; no page is deleted while
    // its native image/player still has a readable lease.
    let queued=try file()
    let queuedTicket=try owner.registerReadOnly(file:queued,generation:1,owner:copies.registration)
    let clear = Task { try await owner.clear(before: 2) }
    while probe.dismiss == nil { await Task.yield() }
    precondition(exists(second) && exists(third) && exists(fourth))
    probe.dismiss?()
    try await clear.value
    do { try await work.value; preconditionFailure("Expected revoked viewer") } catch {}
    precondition(!exists(second) && !exists(third) && !exists(fourth))
    precondition(!exists(queued))
    do {_ = try copies.accept(queuedTicket,key:"queued");preconditionFailure("Accepted cleared queued transfer")}catch{}
    let late = try file()
    do { _ = try owner.registerReadOnly(file:late,generation:1,owner:copies.registration); preconditionFailure("Admitted a retired owner") } catch {}
    precondition(!exists(late))
    let successor = try file()
    let reused = owner.register(file:successor,generation:2)
    do {
      try await owner.open(id:reused,title:"QA",edit:nil,readOnlyCopies:copies){_ in ConversationProbe()}
      preconditionFailure("Rebound old copy owner")
    } catch {}
    precondition(!exists(successor))
    let normal=try file(),pending=try file(),normalCopies=ZoenFilePreview.ReadOnlyCopies(),normalProbe=ConversationProbe()
    let normalTicket=try owner.registerReadOnly(file:normal,generation:2,owner:normalCopies.registration)
    let normalWork=Task{try await owner.open(id:normalTicket,title:"QA",edit:nil,readOnlyCopies:normalCopies){_ in normalProbe}}
    while normalProbe.ended == nil {await Task.yield()}
    let pendingTicket=try owner.registerReadOnly(file:pending,generation:2,owner:normalCopies.registration)
    normalProbe.ended?(nil);try await normalWork.value
    precondition(!exists(normal) && !exists(pending))
    do{_ = try normalCopies.accept(pendingTicket,key:"queued");preconditionFailure("Retired transfer survived close")}catch{}
    let tooLate=try file()
    do{_ = try owner.registerReadOnly(file:tooLate,generation:2,owner:normalCopies.registration);preconditionFailure("Producer registered after close")}catch{}
    precondition(!exists(tooLate))
    try requestContracts()
    print("PASS: owned initial+neighbor eviction, dismissal barrier, revocation, late copy cleanup, signed request boundaries")
  }
  private static func requestContracts() throws {
    let group = "album_" + String(repeating: "a", count: 64)
    func request(_ ids: [String]) throws -> ZoenConversationGalleryRequest {
      let value: [String: Any] = ["spaceId": "space", "selectedId": "b", "entries": ids.map { ["messageId": $0, "itemId": "file-"+$0, "version": 2] }]
      return try .decode(String(decoding: JSONSerialization.data(withJSONObject: value), as: UTF8.self))
    }
    func message(_ id: String, index: Int, author: String = "author", deleted: Bool = false) throws -> ZoenConversationGalleryRequest.Message {
      let value: [String: Any] = ["id": id, "author": ["id": author, "name": "QA"], "deleted": deleted, "forwarded": false, "voice": NSNull(),
        "kind": ["Message": ["text": id, "card": ["kind_id": "file", "item_id": "file-"+id, "version": 2]]],
        "media": ["presentation": "media", "group": ["id": group, "index": index, "count": 2]], "in_thread": "root"]
      return try .init(json: String(decoding: JSONSerialization.data(withJSONObject: value), as: UTF8.self), expected: .init(messageId: id, itemId: "file-"+id, version: 2))
    }
    let value = try request(["b", "a"]), a = try message("a", index: 0), b = try message("b", index: 1)
    let ordered = try value.reconstruct([b,a]).map(\.reference.messageId)
    precondition(ordered == ["a","b"])
    func reject(_ operation: () throws -> Void) { do { try operation(); preconditionFailure("Expected rejected signed membership") } catch {} }
    reject { _ = try value.reconstruct([a, message("b", index: 1, author: "foreign")]) }
    reject { _ = try value.reconstruct([a, message("b", index: 0)]) }
    reject { _ = try message("b", index: 1, deleted: true) }
    reject { _ = try request(Array(repeating: "b", count: 11)) }
    reject { _ = try value.reconstruct([a]) }
  }
}
