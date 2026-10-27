import ExpoModulesCore
import Foundation
import UIKit
import PDFKit
#if canImport(FoundationModels)
import FoundationModels
#endif

extension ZoenSession {
  struct DraftMediaEditAttempt {
    let file: URL
    let revision: String
    let content: String
    let asset: MediaDraftAssetInput
  }
  struct DraftMediaEditBinding {
    let engine: Client
    let generation: UInt64
    let scope: ZoenMediaDraftBridge.Scope
    let kind: ZoenMediaEditCapture.Kind
    let gate: ZoenMediaEditGate
    var revision: String
    var assetId: String
    var mutationId: String
    var sha256: String
    var attempt: DraftMediaEditAttempt?
  }
  nonisolated func activateDraftMediaEdits() { draftMediaEditLifetime.activate() }
  nonisolated func retireDraftMediaEdits() {
    // Native lifecycle admission is synchronous; delayed JS AppState cannot save a stale copy.
    let tokens = draftMediaEditLifetime.retire()
    Task { await self.cancelRetiredDraftMediaEdits(tokens) }
  }
  private func cancelRetiredDraftMediaEdits(_ tokens: [UUID]) {
    for token in tokens { draftPreviewTasks[token]?.cancel() }
  }

  func editMediaDraftAsset(_ request: String, presentationId: String, presenter: UIViewController?) async throws -> String {
    guard let token = UUID(uuidString: presentationId), draftPreviewTasks[token] == nil else {
      throw ZoenMediaFiles.Failure.invalidFile
    }
    if let canceled = cancelledDraftPreviews.firstIndex(of: token) {
      cancelledDraftPreviews.remove(at: canceled)
      throw CancellationError()
    }
    let value = try ZoenMediaDraftBridge.decode(request)
    let engine = try mediaDraftCore(value.scope)
    guard let revision = value.expectedRevision, let assetId = value.assetId else { throw ZoenMediaFiles.Failure.invalidFile }
    let draft = try engine.mediaDraft(scope: value.scope.core)
    let content = try ZoenMediaDraftBridge.content(draft.contentJson)
    guard draft.revision == revision, !draft.attempted, content?["kind"] as? String == "attachments",
          let entries = content?["items"] as? [[String: Any]],
          let selected = entries.first(where: { $0["asset_id"] as? String == assetId }),
          let mutationId = selected["mutation_id"] as? String,
          let asset = draft.assets.first(where: { $0.id == assetId }),
          let kind = ZoenMediaEditCapture.kind(mime: asset.mime) else { throw ZoenFilePreview.Failure.changed }
    let gate = ZoenMediaEditGate()
    try draftMediaEditLifetime.register(token, gate: gate)
    defer { draftMediaEditLifetime.remove(token); draftMediaEdits.removeValue(forKey: token); draftPreviewTasks.removeValue(forKey: token) }
    let bytes = try engine.mediaDraftAsset(scope: value.scope.core, expectedRevision: revision, assetId: assetId)
    let uri = try mediaFiles.stage(bytes, name: asset.name)
    let file: ZoenPreviewFile
    do { file = try mediaFiles.takeForPreview(uri) }
    catch { mediaFiles.release(uri); throw error }
    let id = filePreview.register(file: file, generation: generation)
    draftMediaEdits[token] = DraftMediaEditBinding(engine: engine, generation: generation, scope: value.scope,
      kind: kind, gate: gate, revision: revision, assetId: assetId, mutationId: mutationId, sha256: asset.sha256)
    let preview = filePreview
    let edit = ZoenFilePreview.MediaEditContext(kind: kind, maximumBytes: ZoenMediaFiles.maximumBytes, authorize: { [weak self] in
      guard let self else { throw CancellationError() }
      try await self.authorizeDraftMediaEdit(token)
    }) { [weak self] file in
      guard let self else { throw CancellationError() }
      try await self.saveDraftMediaEdit(file, token: token)
    }
    let task = Task { @MainActor in
      let resumeGallery = ZoenAttachmentGalleryHost.yieldForChildPresentation()
      defer { resumeGallery() }
      try await preview.open(id: id, title: asset.name, edit: nil, mediaEdit: edit) { editing in
        if kind == .image, let editing {
          ZoenAttachmentPhotoEditor(presenter: presenter, editing: editing)
        } else if kind == .video, let editing {
          ZoenAttachmentVideoEditor(presenter: presenter, editing: editing)
        } else {
          ZoenQuickLookPreview(presenter: presenter, editing: editing)
        }
      }
    }
    draftPreviewTasks[token] = task
    do { try await task.value }
    catch is CancellationError {}
    catch ZoenFilePreview.Failure.changed {}
    // Return the actual current journal even if Cancel followed an already accepted explicit Save.
    // No copied UI projection is allowed to restore an older original over that canonical ACK.
    guard self.core === engine else { throw SessionError.closed }
    _ = try mediaDraftCore(value.scope)
    return try ZoenMediaFiles.json(ZoenMediaDraftBridge.object(engine.mediaDraft(scope: value.scope.core)))
  }

  /// Render checkpoints consult the same exact canonical selection that owns
  /// the CAS; a renamed/reordered/replaced item is never silently rebound.
  private func authorizeDraftMediaEdit(_ token: UUID) throws {
    guard let binding = draftMediaEdits[token], !erasing, !closing,
          generation == binding.generation, self.core === binding.engine else { throw SessionError.closed }
    _ = try mediaDraftCore(binding.scope)
    try binding.gate.admitted {
      let draft = try binding.engine.mediaDraft(scope: binding.scope.core)
      guard draft.revision == binding.revision, !draft.attempted, binding.attempt == nil,
            let content = try ZoenMediaDraftBridge.content(draft.contentJson),
            content["kind"] as? String == "attachments",
            let entries = content["items"] as? [[String: Any]],
            entries.contains(where: { $0["asset_id"] as? String == binding.assetId && $0["mutation_id"] as? String == binding.mutationId }),
            draft.assets.contains(where: { $0.id == binding.assetId && $0.sha256 == binding.sha256 }) else {
        throw ZoenFilePreview.Failure.changed
      }
    }
  }

  private func saveDraftMediaEdit(_ file: ZoenPreviewFile, token: UUID) async throws {
    guard let original = draftMediaEdits[token] else { throw SessionError.closed }
    _ = try original.gate.admitted { () }
    let metadata = try await ZoenMediaEditCapture.validate(file, kind: original.kind, maximumBytes: ZoenMediaFiles.maximumBytes)
    guard var binding = draftMediaEdits[token], !erasing, !closing,
          generation == binding.generation, self.core === binding.engine else { throw SessionError.closed }
    _ = try mediaDraftCore(binding.scope)
    try binding.gate.admitted {
      let latest = try binding.engine.mediaDraft(scope: binding.scope.core)
      // The exact input revision remains admissible only for this operation's lost-ACK retry.
      if binding.attempt == nil {
        guard latest.revision == binding.revision, !latest.attempted,
              let content = try ZoenMediaDraftBridge.content(latest.contentJson),
              content["kind"] as? String == "attachments",
              let entries = content["items"] as? [[String: Any]],
              entries.contains(where: { $0["asset_id"] as? String == binding.assetId
                && $0["mutation_id"] as? String == binding.mutationId }),
              let old = latest.assets.first(where: { $0.id == binding.assetId && $0.sha256 == binding.sha256 }) else {
          throw ZoenFilePreview.Failure.changed
        }
        let data = try Data(contentsOf: file.url, options: [.mappedIfSafe])
        guard !data.isEmpty, UInt64(data.count) <= ZoenMediaFiles.maximumBytes else { throw ZoenMediaFiles.Failure.tooLarge }
        let assetId = UUID().uuidString
        let mutationId = UUID().uuidString
        let stem = (old.name as NSString).deletingPathExtension
        let name = String(stem.prefix(50)) + "." + metadata.fileExtension
        let replaced = try ZoenMediaDraftEditedContent.replacingAsset(in: content,
          assetId: binding.assetId, mutationId: binding.mutationId,
          replacementAssetId: assetId, replacementMutationId: mutationId)
        let json = try ZoenMediaFiles.json(replaced)
        binding.attempt = DraftMediaEditAttempt(file: file.url, revision: binding.revision, content: json,
          asset: MediaDraftAssetInput(id: assetId, name: name, mime: metadata.mime, bytes: data))
        // Retain the exact fingerprint inputs BEFORE native CAS. Retry never changes IDs or bytes.
        draftMediaEdits[token] = binding
      }
      guard let attempt = binding.attempt, attempt.file == file.url else { throw ZoenFilePreview.Failure.changed }
      let saved = try binding.engine.saveMediaDraft(scope: binding.scope.core, expectedRevision: attempt.revision,
        contentJson: attempt.content, assets: [attempt.asset])
      guard saved.acceptedRevision == saved.draft.revision,
            let accepted = saved.draft.assets.first(where: { $0.id == attempt.asset.id }),
            let content = try ZoenMediaDraftBridge.content(saved.draft.contentJson),
            let entries = content["items"] as? [[String: Any]],
            let entry = entries.first(where: { $0["asset_id"] as? String == accepted.id }),
            let mutationId = entry["mutation_id"] as? String else { throw ZoenFilePreview.Failure.changed }
      binding.revision = saved.acceptedRevision
      binding.assetId = accepted.id
      binding.mutationId = mutationId
      binding.sha256 = accepted.sha256
      binding.attempt = nil
      draftMediaEdits[token] = binding
    }
  }

  func cancelMediaDraftPreview(_ presentationId: String) async throws {
    guard let token = UUID(uuidString: presentationId) else { throw ZoenMediaFiles.Failure.invalidFile }
    draftMediaEditLifetime.cancel(token)
    guard let task = draftPreviewTasks[token] else {
      if !cancelledDraftPreviews.contains(token) { cancelledDraftPreviews.append(token) }
      if cancelledDraftPreviews.count > 64 { cancelledDraftPreviews.removeFirst() }
      return
    }
    task.cancel()
    do { try await task.value }
    catch is CancellationError {}
    catch ZoenFilePreview.Failure.changed {}
  }

  func sendMediaDraft(_ request: String) throws -> String {
    let value = try ZoenMediaDraftBridge.decode(request)
    let core = try mediaDraftCore(value.scope)
    guard let revision = value.expectedRevision else { throw ZoenMediaFiles.Failure.invalidFile }
    return try core.sendMediaDraftJson(scope: value.scope.core, expectedRevision: revision)
  }

}
