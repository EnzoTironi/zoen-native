import Foundation

/// Real Client/SQLite/encrypted-journal checks. Link with the same regenerated
/// ClientFFI and ENRM module as the app; no runtime/storage substitute is used.
private final class PageInputTestVault: SecretVault, @unchecked Sendable {
  private let lock = NSLock()
  private var values: [String: Data] = [:]
  func load(key: String) -> Data? { lock.lock(); defer { lock.unlock() }; return values[key] }
  func save(key: String, value: Data) -> Bool { lock.lock(); defer { lock.unlock() }; values[key] = value; return true }
  func delete(key: String) { lock.lock(); defer { lock.unlock() }; values.removeValue(forKey: key) }
}

@main private struct PageInputLeaseTests {
  private static func check(_ condition: Bool, _ message: String) {
    if !condition { fatalError(message) }
  }
  private static func reject(_ action: () throws -> Void) {
    do { try action(); fatalError("Expected a closed/stale/malformed input to be rejected") }
    catch { }
  }
  private static func payload(sequence: UInt64, text: String, composition: [String: Int]? = nil) throws -> Data {
    try JSONSerialization.data(withJSONObject: ["schema": "page-text/1", "mode": "markdown",
      "sequence": String(sequence), "text": text,
      "selection": ["start": text.utf16.count, "end": text.utf16.count],
      "composition": composition as Any? ?? NSNull()], options: [.sortedKeys])
  }
  static func main() throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent("zoen-page-input-lease-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
    defer { try? FileManager.default.removeItem(at: directory) }
    let vault = PageInputTestVault()
    let core = try Client.open(path: directory.appendingPathComponent("client.sqlite").path, locale: "en")
    let account = try core.createAccount(name: "Synthetic Page input", handle: "page_\(UUID().uuidString.lowercased().replacingOccurrences(of: "-", with: "").prefix(12))",
      relayUrl: "http://127.0.0.1:1", vault: vault)
    let space = try core.createGroupWith(title: "Synthetic input lease", memberIds: [], privacy: .closed)
    let item = try core.pageImportMarkdown(spaceId: space, path: "input.md", markdown: "Original")
    let page = try core.page(itemId: item.id)
    let scope = FileEditDraftScope(expectedIdentityId: account.identityId, expectedDeviceId: account.deviceId, spaceId: space, itemId: item.id)
    let loaded = try core.pageInputLoad(scope: scope)
    let opened = try core.pageInputOpen(scope: scope, expectedRevision: loaded.revision, writerId: UUID().uuidString,
      target: .block(blockId: page.blocks[0].id), mode: .markdown, observedEditContext: page.editContext)
    let lease = try ZoenPageInputLease(id: UUID().uuidString, engine: core, generation: 7,
      scope: scope, opened: opened, changed: { _ in })
    check(try lease.initialSnapshot() == nil, "New input must have no invented recovery bytes")
    let before = core.connection().pending
    let text = "A🪴 **private input**"
    let capture = try payload(sequence: 1, text: text, composition: ["start": 1, "end": 3])
    _ = try lease.retainDraftSnapshot(capture, sequence: 1)
    let retained = try core.pageInputLoad(scope: scope)
    check(retained.capture?.payload == capture, "The real encrypted journal must own exact text, selection and IME bytes")
    check(try core.pageMarkdown(itemId: item.id) == "Original", "Checkpoint must not Apply or Share")
    check(core.connection().pending == before, "Raw input must not publish an outbox event")
    try lease.didSeal(); try lease.prepare(.apply)
    let accepted = try lease.apply()
    let replay = try lease.apply()
    check(accepted.edit.mutationId == replay.edit.mutationId && accepted.edit.appliedContentHash == replay.edit.appliedContentHash,
      "Lost bridge response must return the same native completed result")
    check(try core.page(itemId: item.id).inputPending == false, "Apply must consume raw capture atomically")
    check(try core.page(itemId: item.id).unsaved, "Apply stays a private Page draft until Share")
    check(core.connection().pending == before, "Apply must not publish the Page")
    lease.invalidate()
    reject { _ = try lease.retainDraftSnapshot(capture, sequence: 2) }

    // A forged UTF-16 range splitting the emoji cannot replace a valid capture.
    let currentPage = try core.page(itemId: item.id)
    let next = try core.pageInputLoad(scope: scope)
    let second = try core.pageInputOpen(scope: scope, expectedRevision: next.revision, writerId: UUID().uuidString,
      target: .block(blockId: currentPage.blocks[0].id), mode: .markdown, observedEditContext: currentPage.editContext)
    let secondLease = try ZoenPageInputLease(id: UUID().uuidString, engine: core, generation: 7,
      scope: scope, opened: second, changed: { _ in })
    _ = try secondLease.initialSnapshot()
    let malformed = try payload(sequence: 1, text: "A🪴B", composition: ["start": 1, "end": 2])
    reject { _ = try secondLease.retainDraftSnapshot(malformed, sequence: 1) }
    let unchanged = try core.pageInputLoad(scope: scope)
    check(unchanged.revision == second.revision && unchanged.capture == nil,
      "Malformed native input must leave the accepted sequence/revision untouched")
    check(secondLease.status().unretained && secondLease.status().failed,
      "A rejected newest snapshot stays native-owned for explicit retry/discard")
    // Discard can release an empty accepted slot without requiring rejected bytes to persist.
    try secondLease.didFreezeForDiscard(); try secondLease.prepare(.discard)
    _ = try secondLease.discard()
    check(try core.pageInputLoad(scope: scope).binding == nil, "Explicit Discard releases the real empty binding")
    check(!secondLease.status().unretained, "Only durable discard ACK may release rejected bytes")
    print("PASS Page input native lease: exact retention, private Apply/replay, invalidate, malformed IME, Discard")
  }
}
