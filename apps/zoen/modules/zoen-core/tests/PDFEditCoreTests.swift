import CoreGraphics
import CryptoKit
import Darwin
import Foundation
import PDFKit

// Run seed/recover as separate processes against a NEW synthetic /private/tmp
// root. The only replacement is presentation; the journal, Swift typed bridge,
// UniFFI, encryption, SQLite, and canonical file mutations are production code.
private enum CorePDFFailure: Error { case assertion(String), unsafeRoot }
private let corePDFMaximum: UInt64 = 64 << 20
private let corePDFMarker = "Zoen.PDFEditCoreTests.v1"
private let corePDFAnnotations = ["Synthetic underline A", "Synthetic underline B"]

private func corePDFCheck(_ condition: Bool, _ message: String) throws {
  if !condition { throw CorePDFFailure.assertion(message) }
}

private func corePDFHash(_ data: Data) -> String {
  SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
}

private func corePDFRegular(_ url: URL, maximum: UInt64) throws -> Data {
  let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
  try corePDFCheck(attributes[.type] as? FileAttributeType == .typeRegular,
                   "Expected an owned regular fixture file")
  try corePDFCheck((attributes[.ownerAccountID] as? NSNumber)?.uint32Value == getuid(),
                   "Fixture file belongs to another user")
  let size = (attributes[.size] as? NSNumber)?.uint64Value ?? UInt64.max
  try corePDFCheck(size <= maximum, "Fixture file exceeds its bound")
  return try Data(contentsOf: url)
}

private func corePDFWrite(_ bytes: Data, to url: URL) throws {
  let manager = FileManager.default
  let temporary = url.deletingLastPathComponent().appendingPathComponent(".write-\(UUID().uuidString)")
  guard manager.createFile(atPath: temporary.path, contents: nil,
                           attributes: [.posixPermissions: 0o600]) else {
    throw CorePDFFailure.assertion("Could not create private fixture file")
  }
  defer { try? manager.removeItem(at: temporary) }
  let handle = try FileHandle(forWritingTo: temporary)
  do { try handle.write(contentsOf: bytes); try handle.synchronize(); try handle.close() }
  catch { try? handle.close(); throw error }
  // POSIX rename replaces one exact owned file without making secrets readable
  // between creation and chmod. No key value is ever encoded in test output.
  guard Darwin.rename(temporary.path, url.path) == 0 else {
    throw CorePDFFailure.assertion("Could not persist private fixture file")
  }
}

private final class CorePDFDiskVault: SecretVault, @unchecked Sendable {
  private let root: URL
  private let lock = NSLock()
  init(root: URL) throws {
    self.root = root
    let attributes = try FileManager.default.attributesOfItem(atPath: root.path)
    try corePDFCheck(attributes[.type] as? FileAttributeType == .typeDirectory,
                     "Vault must be an owned directory")
    try corePDFCheck((attributes[.posixPermissions] as? NSNumber)?.intValue == 0o700,
                     "Vault permissions must be 0700")
  }
  private func location(_ key: String) -> URL? {
    guard !key.isEmpty, key.utf8.count <= 512 else { return nil }
    return root.appendingPathComponent(corePDFHash(Data(key.utf8)))
  }
  func load(key: String) -> Data? {
    lock.lock(); defer { lock.unlock() }
    guard let url = location(key),
          let attributes = try? FileManager.default.attributesOfItem(atPath: url.path),
          (attributes[.posixPermissions] as? NSNumber)?.intValue == 0o600 else { return nil }
    return try? corePDFRegular(url, maximum: 64 << 10)
  }
  func save(key: String, value: Data) -> Bool {
    lock.lock(); defer { lock.unlock() }
    guard let url = location(key), value.count <= 64 << 10 else { return false }
    do { try corePDFWrite(value, to: url); return true } catch { return false }
  }
  func delete(key: String) {
    lock.lock(); defer { lock.unlock() }
    if let url = location(key) { try? FileManager.default.removeItem(at: url) }
  }
}

// This document contains public identifiers and hashes only, never draft bytes,
// vault contents, private keys, or a portable account backup.
private struct CorePDFState: Codable {
  let format: String
  let conflict: Bool
  let identityID: String
  let deviceID: String
  let spaceID: String
  let itemID: String
  let originVersion: UInt32
  let originHash: String
  let editorID: String
  let revision: String
  let captureIDs: [String]
  let hashes: [String]
  let sizes: [UInt64]
  let canonicalVersion: UInt32
  let canonicalHash: String
  let events: UInt64
  let headHash: String
  let pendingEvents: UInt64

  var scope: FileEditDraftScope {
    .init(expectedIdentityId: identityID, expectedDeviceId: deviceID, spaceId: spaceID, itemId: itemID)
  }
  var origin: ZoenFilePreview.EditBase { .init(version: originVersion, sha256: originHash) }
}

private struct CorePDFFixture {
  let root: URL
  var database: URL { root.appendingPathComponent("device.sqlite") }
  var stateURL: URL { root.appendingPathComponent("public-state.json") }
  var leases: URL { root.appendingPathComponent("leases", isDirectory: true) }
  var vaultURL: URL { root.appendingPathComponent("vault", isDirectory: true) }

  init(path: String, create: Bool) throws {
    // Check the raw, single-component name before constructing a URL. URL
    // equality includes Foundation's directory/normalization representation;
    // it is not a filesystem ancestry check.
    let prefix = "/private/tmp/"
    guard path.hasPrefix(prefix) else { throw CorePDFFailure.unsafeRoot }
    let name = String(path.dropFirst(prefix.count))
    guard name.hasPrefix("zoen-pdf-edit-core-"), name.utf8.count <= 120,
          name.utf8.allSatisfy({
            (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) || $0 == 45
          }) else { throw CorePDFFailure.unsafeRoot }
    let temporary = URL(fileURLWithPath: "/private/tmp", isDirectory: true).resolvingSymlinksInPath()
    root = temporary.appendingPathComponent(name, isDirectory: true)
    let manager = FileManager.default
    if create {
      guard !manager.fileExists(atPath: root.path) else { throw CorePDFFailure.unsafeRoot }
      try manager.createDirectory(at: root, withIntermediateDirectories: false,
                                  attributes: [.posixPermissions: 0o700])
      try manager.createDirectory(at: vaultURL, withIntermediateDirectories: false,
                                  attributes: [.posixPermissions: 0o700])
    }
    let attributes = try manager.attributesOfItem(atPath: root.path)
    guard attributes[.type] as? FileAttributeType == .typeDirectory,
          (attributes[.ownerAccountID] as? NSNumber)?.uint32Value == getuid(),
          (attributes[.posixPermissions] as? NSNumber)?.intValue == 0o700 else {
      throw CorePDFFailure.unsafeRoot
    }
  }

  func lease(_ data: Data) throws -> ZoenPreviewFile {
    let manager = FileManager.default
    try manager.createDirectory(at: leases, withIntermediateDirectories: true,
                                attributes: [.posixPermissions: 0o700])
    let directory = leases.appendingPathComponent(UUID().uuidString, isDirectory: true)
    try manager.createDirectory(at: directory, withIntermediateDirectories: false,
                                attributes: [.posixPermissions: 0o700])
    let file = ZoenPreviewFile(url: directory.appendingPathComponent("synthetic.pdf"), directory: directory)
    do { try corePDFWrite(data, to: file.url) }
    catch { try? file.remove(); throw error }
    return file
  }

  func finishLeases() throws {
    let manager = FileManager.default
    if manager.fileExists(atPath: leases.path) {
      try corePDFCheck(try manager.contentsOfDirectory(atPath: leases.path).isEmpty,
                       "A plaintext PDF lease survived phase completion")
      try manager.removeItem(at: leases)
    }
    let names = try manager.contentsOfDirectory(atPath: root.path)
    try corePDFCheck(names.allSatisfy {
      ["vault", "public-state.json", "device.sqlite", "device.sqlite-wal", "device.sqlite-shm"].contains($0)
    }, "Unexpected plaintext fixture output")
  }
}

private func corePDFOriginal() throws -> Data {
  let output = NSMutableData()
  var box = CGRect(x: 0, y: 0, width: 300, height: 200)
  guard let consumer = CGDataConsumer(data: output as CFMutableData),
        let context = CGContext(consumer: consumer, mediaBox: &box,
          [kCGPDFContextTitle as String: "Synthetic PDF edit core fixture"] as CFDictionary) else {
    throw CorePDFFailure.assertion("Could not make synthetic PDF")
  }
  context.beginPDFPage(nil)
  context.setFillColor(gray: 0.2, alpha: 1)
  context.fill(CGRect(x: 30, y: 110, width: 210, height: 2))
  context.fill(CGRect(x: 30, y: 70, width: 210, height: 2))
  context.endPDFPage(); context.closePDF()
  return Data(bytes: output.bytes, count: output.length)
}

private func corePDFAnnotate(_ original: Data, contents: [String]) throws -> Data {
  guard let document = PDFDocument(data: original), let page = document.page(at: 0),
        document.pageCount == 1, !document.isLocked else {
    throw CorePDFFailure.assertion("Synthetic PDF is invalid")
  }
  for (index, content) in contents.enumerated() {
    let annotation = PDFAnnotation(bounds: CGRect(x: 30, y: CGFloat(104 - index * 40), width: 210, height: 12),
                                   forType: .underline, withProperties: nil)
    annotation.contents = content
    page.addAnnotation(annotation)
  }
  guard let result = document.dataRepresentation() else {
    throw CorePDFFailure.assertion("Could not encode synthetic annotations")
  }
  return result
}

private func corePDFValidate(_ bytes: Data, annotations: [String]) throws {
  guard let document = PDFDocument(data: bytes), document.pageCount == 1,
        !document.isLocked, let page = document.page(at: 0) else {
    throw CorePDFFailure.assertion("Recovered PDF is not a readable one-page document")
  }
  try corePDFCheck(page.annotations.map(\.contents) == annotations.map(Optional.some),
                   "Recovered annotation contents/order changed")
  let underlineType = PDFAnnotation(bounds: .zero, forType: .underline, withProperties: nil).type
  try corePDFCheck(page.annotations.allSatisfy { $0.type == underlineType },
                   "Recovered annotation subtype changed")
}

@MainActor private final class CorePDFSignal {
  private var count = 0
  private var waiting: [(Int, CheckedContinuation<Void, Never>)] = []
  func emit() {
    count += 1
    let ready = waiting.filter { $0.0 <= count }
    waiting.removeAll { $0.0 <= count }
    ready.forEach { $0.1.resume() }
  }
  func wait(_ target: Int = 1) async {
    if count < target { await withCheckedContinuation { waiting.append((target, $0)) } }
  }
}

@MainActor private final class CorePDFPresentation: ZoenFilePreviewPresentation {
  let shown = CorePDFSignal()
  let recovery = CorePDFSignal()
  var ended: (@MainActor (Error?) -> Void)?
  var retry: (@MainActor () -> Void)?
  var keep: (@MainActor () -> Void)?
  var discard: (@MainActor () -> Void)?
  var error: Error?
  var changesSaved = false
  var shownURL: URL?
  func show(file: URL, title: String, ended: @escaping @MainActor (Error?) -> Void) throws {
    shownURL = file; self.ended = ended; shown.emit()
  }
  func dismiss(completion: @escaping @MainActor () -> Void) { completion() }
  func recover(error: Error, changesSaved: Bool, retry: (@MainActor () -> Void)?,
               keep: (@MainActor () -> Void)?, discard: @escaping @MainActor () -> Void) {
    self.error = error; self.changesSaved = changesSaved
    self.retry = retry; self.keep = keep; self.discard = discard
    recovery.emit()
  }
}

@main @MainActor struct PDFEditCoreTests {
  static func main() async throws {
    let arguments = CommandLine.arguments
    guard arguments.count == 3,
          ["seed", "seed-conflict", "recover-save", "recover-keep"].contains(arguments[1]) else {
      throw CorePDFFailure.assertion("Use seed|recover-save|seed-conflict|recover-keep ROOT")
    }
    let phase = arguments[1]
    let fixture = try CorePDFFixture(path: arguments[2], create: phase.hasPrefix("seed"))
    if phase.hasPrefix("seed") { try seed(fixture, conflict: phase == "seed-conflict") }
    else { try await recover(fixture, conflict: phase == "recover-keep") }
    print("PDFEditCoreTests \(phase): PASS")
  }

  private static func seed(_ fixture: CorePDFFixture, conflict: Bool) throws {
    let vault = try CorePDFDiskVault(root: fixture.vaultURL)
    let core = try RodaEngine.open(path: fixture.database.path, locale: "en")
    let account = try core.createAccount(name: "Synthetic PDF editor", handle: "pdf_core_\(UUID().uuidString.lowercased().replacingOccurrences(of: "-", with: "").prefix(12))",
                                         relayUrl: "http://127.0.0.1:1", vault: vault)
    let space = try core.createGroupWith(title: "Synthetic PDF journal", memberIds: [], privacy: .closed)
    let original = try corePDFOriginal()
    let a = try corePDFAnnotate(original, contents: Array(corePDFAnnotations.prefix(1)))
    let b = try corePDFAnnotate(original, contents: corePDFAnnotations)
    try corePDFValidate(a, annotations: Array(corePDFAnnotations.prefix(1)))
    try corePDFValidate(b, annotations: corePDFAnnotations)
    let item = try core.fileAdd(spaceId: space, path: "synthetic.pdf", name: "synthetic.pdf",
                                 mime: "application/pdf", bytes: original, thumbnail: nil)
    guard let info = item.file else { throw CorePDFFailure.assertion("Original PDF is missing") }
    let scope = FileEditDraftScope(expectedIdentityId: account.identityId, expectedDeviceId: account.deviceId,
                                   spaceId: space, itemId: item.id)
    let origin = ZoenFilePreview.EditBase(version: item.version, sha256: info.sha256)
    let journal = try ZoenPDFEditCore.make(engine: core, scope: scope, observedBase: origin, maximumBytes: corePDFMaximum)
    let before = core.verifyLog(spaceId: space)
    let pendingBefore = core.connection().pending
    let first = try fixture.lease(a)
    defer { try? first.remove() }
    let second = try fixture.lease(b)
    defer { try? second.remove() }
    try journal.retain(first); try journal.retain(second)
    let staged = try core.fileEditDraft(scope: scope)
    let unchanged = core.verifyLog(spaceId: space)
    try corePDFCheck(item.version == 1 && before.valid && unchanged == before
                     && core.connection().pending == pendingBefore,
                     "Private retain appended a public event")
    try corePDFCheck(try core.item(itemId: item.id) == item, "Private retain changed canonical file")
    try corePDFCheck(staged.pending.count == 2 && staged.pending.map(\.sha256) == [corePDFHash(a), corePDFHash(b)],
                     "Two complete captures were not retained in FIFO order")
    try corePDFCheck(staged.editorId == journal.editorID && staged.originVersion == 1 && staged.baseVersion == 1,
                     "Private journal lost its editor/original checked base")
    var canonical = item
    if conflict {
      let external = try corePDFAnnotate(original, contents: ["Synthetic independent canonical edit"])
      canonical = try core.fileNewVersion(itemId: item.id, bytes: external, thumbnail: nil,
                                          note: "Independent synthetic edit")
      try corePDFCheck(canonical.version == 2 && canonical.file?.sha256 != corePDFHash(b),
                       "Could not establish real canonical conflict")
    }
    let log = core.verifyLog(spaceId: space)
    guard let canonicalFile = canonical.file else { throw CorePDFFailure.assertion("Canonical file disappeared") }
    let state = CorePDFState(format: corePDFMarker, conflict: conflict, identityID: account.identityId,
      deviceID: account.deviceId, spaceID: space, itemID: item.id, originVersion: origin.version,
      originHash: origin.sha256, editorID: journal.editorID, revision: staged.revision,
      captureIDs: staged.pending.map(\.captureId), hashes: staged.pending.map(\.sha256),
      sizes: staged.pending.map(\.size), canonicalVersion: canonical.version,
      canonicalHash: canonicalFile.sha256, events: log.events, headHash: log.headHash,
      pendingEvents: core.connection().pending)
    try corePDFWrite(JSONEncoder().encode(state), to: fixture.stateURL)
    try first.remove(); try second.remove()
    try fixture.finishLeases()
    // The process exits with no PDF file outside encrypted storage. Public JSON
    // has hashes/identifiers only; raw captures are never exported for recovery.
    let publicMetadata = try corePDFRegular(fixture.stateURL, maximum: 64 << 10)
    try corePDFCheck(publicMetadata.range(of: a) == nil && publicMetadata.range(of: b) == nil,
                     "Public fixture metadata contains private PDF bytes")
  }

  private static func recover(_ fixture: CorePDFFixture, conflict: Bool) async throws {
    let state = try JSONDecoder().decode(CorePDFState.self,
      from: corePDFRegular(fixture.stateURL, maximum: 64 << 10))
    try corePDFCheck(state.format == corePDFMarker && state.conflict == conflict,
                     "Recovery phase does not match this owned fixture")
    let vault = try CorePDFDiskVault(root: fixture.vaultURL)
    let core = try RodaEngine.open(path: fixture.database.path, locale: "en")
    try corePDFCheck(try core.unlock(vault: vault), "Could not unlock the synthetic account")
    try corePDFCheck(core.account()?.identityId == state.identityID && core.account()?.deviceId == state.deviceID,
                     "Cold recovery opened a different account/device")
    let before = core.verifyLog(spaceId: state.spaceID)
    try corePDFCheck(before.valid && before.events == state.events && before.headHash == state.headHash
                     && core.connection().pending == state.pendingEvents,
                     "Public log changed between seed and recovery")
    let current = try core.item(itemId: state.itemID)
    try corePDFCheck(current.version == state.canonicalVersion && current.file?.sha256 == state.canonicalHash,
                     "Canonical PDF changed before recovery")
    let observed = ZoenFilePreview.EditBase(version: current.version, sha256: state.canonicalHash)
    let journal = try ZoenPDFEditCore.make(engine: core, scope: state.scope, observedBase: observed,
                                          maximumBytes: corePDFMaximum)
    let recovered = journal.recovered
    try corePDFCheck(recovered.revision == state.revision && journal.editorID == state.editorID
                     && journal.origin == state.origin && journal.initialBase == state.origin,
                     "Cold recovery rebased or replaced the captured editor")
    try corePDFCheck(recovered.pending.map(\.id) == state.captureIDs
                     && recovered.pending.map(\.sha256) == state.hashes
                     && recovered.pending.map(\.size) == state.sizes,
                     "Cold recovery changed the exact FIFO captures")
    var files: [ZoenPreviewFile] = []
    defer { files.forEach { try? $0.remove() } }
    var bytes: [Data] = []
    for (index, capture) in recovered.pending.enumerated() {
      let data = try journal.recoveredBytes(capture)
      try corePDFValidate(data, annotations: Array(corePDFAnnotations.prefix(index + 1)))
      let file = try fixture.lease(data)
      files.append(file)
      try journal.registerRecovered(file, capture: capture)
      bytes.append(data)
    }
    try corePDFCheck(files.count == 2, "Recovery did not return both captures")
    let viewer = try fixture.lease(bytes[1])
    defer { try? viewer.remove() }
    let owner = ZoenFilePreview()
    let presentation = CorePDFPresentation()
    let context = ZoenFilePreview.EditContext(base: journal.initialBase, maximumBytes: corePDFMaximum,
      retain: { try journal.retain($0) }, discard: { try journal.discard() },
      save: { file, _ in try journal.save(file) })
    let transfer = owner.register(file: viewer, generation: 1, recovered: files)
    let task = Task { @MainActor in
      try await owner.open(id: transfer, title: "Synthetic recovered PDF", edit: context) { _ in presentation }
    }
    await presentation.shown.wait()
    try corePDFCheck(presentation.shownURL == viewer.url && viewer.url != files[1].url,
                     "Viewer must own a separate lease of the newest capture")
    try corePDFCheck(core.verifyLog(spaceId: state.spaceID) == before
                     && core.connection().pending == state.pendingEvents,
                     "Recovered presentation automatically published changes")
    try corePDFCheck(try core.fileEditDraft(scope: state.scope).pending.map(\.captureId) == state.captureIDs,
                     "Presentation consumed a recovered capture")
    presentation.ended?(nil)
    await presentation.recovery.wait()
    try corePDFCheck(!presentation.changesSaved && presentation.retry != nil
                     && presentation.keep != nil && presentation.discard != nil,
                     "Recovered edits need explicit Save, Keep, and Discard choices")
    presentation.retry?()
    if conflict {
      await presentation.recovery.wait(2)
      let afterFailure = try core.fileEditDraft(scope: state.scope)
      try corePDFCheck(afterFailure.pending.map(\.captureId) == state.captureIDs
                       && afterFailure.revision == state.revision,
                       "Stale Save consumed private captures")
      var staleReason: String?
      do {
        _ = try core.commitFileEditDraft(scope: state.scope, editorId: state.editorID,
                                         captureId: state.captureIDs[0], note: "Synthetic conflict check")
      } catch CoreError.Stale(let reason) { staleReason = reason }
      try corePDFCheck(staleReason != nil && presentation.error?.localizedDescription == staleReason,
                       "Production bridge did not surface the real checked-base conflict")
      try corePDFCheck(!presentation.changesSaved && presentation.keep != nil,
                       "Stale private edits must remain available for Keep for Later")
      try corePDFValidate(bytes[1], annotations: corePDFAnnotations)
      presentation.keep?()
      try await task.value
      try corePDFCheck(try core.fileEditDraft(scope: state.scope) == afterFailure,
                       "Keep for Later discarded or restaged private captures")
      try corePDFCheck(core.verifyLog(spaceId: state.spaceID) == before
                       && core.connection().pending == state.pendingEvents,
                       "Stale Save or Keep published a canonical event")
      try corePDFCheck(try core.item(itemId: state.itemID).version == 2,
                       "Stale Save overwrote the unseen canonical version")
    } else {
      try await task.value
      let result = try core.item(itemId: state.itemID)
      let empty = try core.fileEditDraft(scope: state.scope)
      try corePDFCheck(result.version == 3 && result.versions.map(\.number) == [3, 2, 1]
                       && result.file?.sha256 == state.hashes[1] && empty.pending.isEmpty,
                       "Explicit Save must publish both complete snapshots exactly once")
      guard let finalBytes = try core.fileBytesBounded(itemId: state.itemID, version: 3, maxBytes: corePDFMaximum),
            let firstBytes = try core.fileBytesBounded(itemId: state.itemID, version: 2, maxBytes: corePDFMaximum) else {
        throw CorePDFFailure.assertion("Saved version bytes are unavailable")
      }
      try corePDFCheck(finalBytes == bytes[1] && firstBytes == bytes[0],
                       "Canonical versions differ from captured complete PDFs")
      try corePDFValidate(finalBytes, annotations: corePDFAnnotations)
      let accepted = core.verifyLog(spaceId: state.spaceID)
      // This real account is intentionally offline. Public mutations enter its
      // durable outbox; verifyLog covers only relay-sequenced events on disk.
      let pendingAccepted = core.connection().pending
      try corePDFCheck(accepted == before && pendingAccepted == state.pendingEvents + 2,
                       "Save appended an unexpected number of canonical events")
      for index in 0..<2 {
        let replay = try core.commitFileEditDraft(scope: state.scope, editorId: state.editorID,
          captureId: state.captureIDs[index], note: "Ignored exact receipt replay")
        try corePDFCheck(replay.version == UInt32(index + 2) && replay.sha256 == state.hashes[index],
                         "Cold commit receipt did not preserve the exact capture result")
      }
      try corePDFCheck(core.verifyLog(spaceId: state.spaceID) == accepted
                       && core.connection().pending == pendingAccepted
                       && core.fileEditDraft(scope: state.scope) == empty,
                       "Accepted receipt replay duplicated a version or changed private state")
    }
    try corePDFCheck(!FileManager.default.fileExists(atPath: viewer.url.path)
                     && files.allSatisfy { !FileManager.default.fileExists(atPath: $0.url.path) },
                     "Finished owner retained a plaintext lease")
    try fixture.finishLeases()
  }
}
