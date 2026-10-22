import CoreGraphics
import CryptoKit
import Darwin
import Foundation
import PDFKit

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

  static func seed(_ fixture: CorePDFFixture, conflict: Bool) throws {
    let vault = try CorePDFDiskVault(root: fixture.vaultURL)
    let core = try Client.open(path: fixture.database.path, locale: "en")
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

  static func recover(_ fixture: CorePDFFixture, conflict: Bool) async throws {
    let state = try JSONDecoder().decode(CorePDFState.self,
      from: corePDFRegular(fixture.stateURL, maximum: 64 << 10))
    try corePDFCheck(state.format == corePDFMarker && state.conflict == conflict,
                     "Recovery phase does not match this owned fixture")
    let vault = try CorePDFDiskVault(root: fixture.vaultURL)
    let core = try Client.open(path: fixture.database.path, locale: "en")
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
