import ExpoModulesCore
import Foundation
import UIKit
import PDFKit
#if canImport(FoundationModels)
import FoundationModels
#endif

extension ZoenSession {
  func fileInfo(itemId: String, version: Int?) throws -> String {
    guard !erasing, !closing, let core else { throw SessionError.closed }
    return try ZoenMediaFiles.json(ZoenMediaFiles.info(try core.fileInfoAt(itemId: itemId, version: ZoenMediaFiles.version(version))))
  }

  func materializeRecentAttachment(_ request: String) async throws -> String {
    guard recentSelections.count < 20 else { throw ZoenMediaFiles.Failure.invalidFile }
    let value = try ZoenRecentAttachmentBridge.decode(request)
    let engine = try mediaDraftCore(value.scope), kind = try value.coreKind
    let reference = value.reference.core, expected = generation
    let row = try engine.attachmentReferenceValidate(scope: value.scope.core, kind: kind, reference: reference)
    let local = try await materializeFile(itemId: reference.itemId, version: Int(reference.version))
    guard let object = try JSONSerialization.jsonObject(with: Data(local.utf8)) as? [String: Any],
          let uri = object["uri"] as? String else { throw ZoenMediaFiles.Failure.invalidFile }
    do {
      let identity = try mediaFiles.ownedSelectionURL(uri).path
      guard !erasing, !closing, self.core === engine, generation == expected,
            recentSelections.count < 20,
            let binding = previewBindings[identity], bindingIsCurrent(binding),
            binding.sha256 == reference.sha256, binding.version == reference.version else { throw SessionError.closed }
      _ = try mediaDraftCore(value.scope)
      let checked = try engine.attachmentReferenceValidate(scope: value.scope.core, kind: kind, reference: reference)
      guard checked.name == row.name, checked.mime == row.mime, checked.bytes == row.bytes else { throw ZoenMediaFiles.Failure.invalidVersion }
      recentSelections[identity] = RecentSelectionBinding(engine: engine, generation: expected, scope: value.scope, kind: kind, reference: reference)
      return try ZoenMediaFiles.json(["uri": uri, "name": row.name, "mime": row.mime, "size": row.bytes])
    } catch { releaseLocalFile(uri); throw error }
  }

  func previewRecentAttachment(_ request: String, presentationId: String, presenter: UIViewController?) async throws {
    guard let token = UUID(uuidString: presentationId), draftPreviewTasks[token] == nil else { throw ZoenMediaFiles.Failure.invalidFile }
    if let canceled = cancelledDraftPreviews.firstIndex(of: token) {
      cancelledDraftPreviews.remove(at: canceled); throw CancellationError()
    }
    let gate = ZoenMediaEditGate()
    try draftMediaEditLifetime.register(token, gate: gate)
    let task = Task { try await self.openRecentAttachmentPreview(request, presenter: presenter, gate: gate) }
    draftPreviewTasks[token] = task
    defer { draftPreviewTasks.removeValue(forKey: token); draftMediaEditLifetime.remove(token) }
    try await task.value
  }

  private func openRecentAttachmentPreview(_ request: String, presenter: UIViewController?, gate: ZoenMediaEditGate) async throws {
    try Task.checkCancellation(); try gate.admitted { () }
    let local = try await materializeRecentAttachment(request)
    guard let object = try JSONSerialization.jsonObject(with: Data(local.utf8)) as? [String: Any],
          let uri = object["uri"] as? String else { throw SessionError.closed }
    let identity = try mediaFiles.ownedSelectionURL(uri).path
    guard let source = recentSelections[identity], let binding = previewBindings[identity], bindingIsCurrent(binding) else { throw SessionError.closed }
    let file: ZoenPreviewFile
    do {
      try Task.checkCancellation(); try gate.admitted { () }
      _ = try source.engine.attachmentReferenceValidate(scope: source.scope.core, kind: source.kind, reference: source.reference)
      file = try mediaFiles.takeForPreview(uri)
    } catch { releaseLocalFile(uri); throw error }
    recentSelections.removeValue(forKey: identity); previewBindings.removeValue(forKey: identity)
    let ticket = filePreview.register(file: file, generation: generation)
    try await filePreview.open(id: ticket, title: binding.name) {
      if ZoenMediaGalleryPreview.supports(mime: binding.mime) {
        return ZoenMediaGalleryPreview(presenter: presenter, mime: binding.mime)
      }
      return ZoenQuickLookPreview(presenter: presenter, editing: nil)
    }
  }

  func materializeFile(itemId: String, version: Int?) async throws -> String {
    guard !erasing, !closing, let core else { throw SessionError.closed }
    // Pin metadata, version and account before any suspension. History opens
    // demand only this version; they never restore it or prefetch all history.
    let expectedGeneration = generation
    let expectedMode = mode
    let item = try core.item(itemId: itemId)
    let scope = try fileScope(spaceId: item.spaceId, engine: core)
    let pinned = try ZoenMediaFiles.version(version) ?? item.version
    var info = try core.fileInfoAt(itemId: itemId, version: pinned)
    let expectedSHA256 = info.sha256
    var bytes = try core.fileBytesBounded(itemId: itemId, version: pinned, maxBytes: ZoenMediaFiles.maximumBytes)
    if bytes == nil {
      _ = try await core.ensureFileVersion(itemId: itemId, version: pinned,
        maxBytes: ZoenMediaFiles.maximumBytes, timeoutMs: 30_000)
      guard !erasing, !closing, self.core === core, generation == expectedGeneration, mode == expectedMode,
            try fileScope(spaceId: item.spaceId, engine: core) == scope else { throw SessionError.closed }
      info = try core.fileInfoAt(itemId: itemId, version: pinned)
      bytes = try core.fileBytesBounded(itemId: itemId, version: pinned, maxBytes: ZoenMediaFiles.maximumBytes)
    }
    guard !erasing, !closing, self.core === core, generation == expectedGeneration, mode == expectedMode,
          try fileScope(spaceId: item.spaceId, engine: core) == scope else { throw SessionError.closed }
    guard let bytes else { throw ZoenMediaFiles.Failure.invalidFile }
    info = try core.fileInfoAt(itemId: itemId, version: pinned)
    guard info.sha256 == expectedSHA256 else { throw ZoenMediaFiles.Failure.invalidVersion }
    let uri = try mediaFiles.stage(bytes, name: info.name)
    previewBindings[try mediaFiles.ownedSelectionURL(uri).path] = PreviewBinding(engine: core, generation: generation, mode: mode,
      scope: scope, itemId: itemId, spaceId: item.spaceId,
      version: pinned, sha256: info.sha256, name: info.name, mime: info.mime)
    return try ZoenMediaFiles.json(["uri": uri, "itemId": itemId, "version": pinned, "info": ZoenMediaFiles.info(info)])
  }

}
