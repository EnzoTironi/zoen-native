import ExpoModulesCore
import Foundation
import UIKit
import PDFKit
#if canImport(FoundationModels)
import FoundationModels
#endif

extension ZoenSession {
  func mediaDraftCore(_ scope: ZoenMediaDraftBridge.Scope) throws -> Client {
    guard !erasing, !closing, mode == "account", let core else { throw SessionError.closed }
    let owner = try fileScope(spaceId: scope.spaceId, engine: core)
    guard owner.identity == scope.expectedIdentityId, owner.device == scope.expectedDeviceId else { throw SessionError.closed }
    return core
  }

  func loadMediaDraft(_ request: String) throws -> String {
    let value = try ZoenMediaDraftBridge.decode(request)
    let core = try mediaDraftCore(value.scope)
    return try ZoenMediaFiles.json(ZoenMediaDraftBridge.object(core.mediaDraft(scope: value.scope.core)))
  }

  func saveMediaDraft(_ request: String) async throws -> String {
    let value = try ZoenMediaDraftBridge.decode(request)
    let core = try mediaDraftCore(value.scope)
    guard let revision = value.expectedRevision, let json = value.contentJson,
          (value.assets?.count ?? 0) <= 10 else { throw ZoenMediaFiles.Failure.invalidFile }
    let expected = generation
    let previous = try core.mediaDraft(scope: value.scope.core)
    let oldContent = try ZoenMediaDraftBridge.content(previous.contentJson)
    let content = try ZoenMediaDraftBridge.content(json)
    var inputs: [MediaDraftAssetInput] = []
    var recentSources: [RecentAttachmentSourceBinding] = []
    var admitted = Set<String>()
    var voiceLeases: [(sourceId: String, renderUri: String?)] = []
    requests += 1
    defer { retireRequest() }
    for asset in value.assets ?? [] {
      guard admitted.insert(asset.id).inserted else { throw ZoenMediaFiles.Failure.invalidFile }
      if let role = asset.voiceRole {
        guard let sourceId = asset.voiceSourceId, voiceSources[sourceId] == expected,
              content?["kind"] as? String == "voice" else { throw ZoenMediaFiles.Failure.invalidRecording }
        switch role {
        case "source":
          guard content?["source_asset_id"] as? String == asset.id, asset.mime.hasPrefix("audio/") else { throw ZoenMediaFiles.Failure.invalidRecording }
          let source = try await voiceEditor.admitSource(sourceId: sourceId, uri: asset.uri)
          try ZoenMediaDraftBridge.sourceMetadata(content!, source: source)
        case "render":
          guard voiceRenders[sourceId] == asset.uri,
                let metadata = content?["render"] as? [String: Any],
                metadata["asset_id"] as? String == asset.id else { throw ZoenMediaFiles.Failure.invalidRecording }
          let render = try await voiceEditor.admitSend(sourceId: sourceId, uri: asset.uri)
          guard asset.mime == render.mime else { throw ZoenMediaFiles.Failure.invalidRecording }
          try ZoenMediaDraftBridge.renderMetadata(metadata, render: render)
        default: throw ZoenMediaFiles.Failure.invalidRecording
        }
        guard voiceSources[sourceId] == expected,
              role != "render" || voiceRenders[sourceId] == asset.uri else { throw SessionError.closed }
        voiceLeases.append((sourceId: sourceId, renderUri: role == "render" ? asset.uri : nil))
      } else if asset.voiceSourceId != nil || content?["kind"] as? String == "voice" {
        throw ZoenMediaFiles.Failure.invalidRecording
      }
      guard !erasing, !closing, generation == expected, self.core === core else { throw SessionError.closed }
      _ = try mediaDraftCore(value.scope)
      let selection = try mediaFiles.readSelection(uri: asset.uri, name: asset.name, mime: asset.mime)
      inputs.append(MediaDraftAssetInput(id: asset.id, name: selection.name, mime: selection.mime, bytes: selection.data))
      if let recent = recentSelections[selection.url.path] {
        guard recent.engine === core, recent.generation == expected, recent.scope == value.scope,
              asset.voiceRole == nil, asset.voiceSourceId == nil else { throw SessionError.closed }
        recentSources.append(RecentAttachmentSourceBinding(assetId: asset.id, kind: recent.kind, reference: recent.reference))
      }
    }
    if previous.revision == revision, content?["kind"] as? String == "voice" {
      // An existing render is immutable too: metadata-only edits cannot relabel its bytes.
      // A stale exact-save retry instead belongs to Rust's authenticated receipt;
      // comparing it with a newer cleared/replaced journal would reject its ACK.
      if let render = content?["render"] as? [String: Any], let id = render["asset_id"] as? String,
         !admitted.contains(id) {
        guard let old = oldContent?["render"] as? [String: Any], old["asset_id"] as? String == id,
              ZoenMediaDraftBridge.sameRender(old, render) else { throw ZoenMediaFiles.Failure.invalidRecording }
      }
      if let sourceId = content?["source_asset_id"] as? String, !admitted.contains(sourceId) {
        let oldEditor = oldContent?["editor"] as? [String: Any]
        let editor = content?["editor"] as? [String: Any]
        guard oldContent?["source_asset_id"] as? String == sourceId,
              ZoenMediaDraftBridge.sameOriginal((oldEditor?["draft"] as? [String: Any])?["original"],
                (editor?["draft"] as? [String: Any])?["original"]) else { throw ZoenMediaFiles.Failure.invalidRecording }
      }
    }
    guard !erasing, !closing, generation == expected, self.core === core else { throw SessionError.closed }
    _ = try mediaDraftCore(value.scope)
    // A later asset's actor hop can revoke a previously admitted source/render.
    for lease in voiceLeases {
      guard voiceSources[lease.sourceId] == expected,
            lease.renderUri.map({ voiceRenders[lease.sourceId] == $0 }) ?? true else { throw SessionError.closed }
    }
    // Source-bound ingestion and its receipt retry happen under the existing
    // canonical journal lock. A separate validation call cannot grant Save.
    let saved = try recentSources.isEmpty
      ? core.saveMediaDraft(scope: value.scope.core, expectedRevision: revision, contentJson: json, assets: inputs)
      : core.saveMediaDraftWithRecentSources(scope: value.scope.core, expectedRevision: revision,
        contentJson: json, assets: inputs, sourceBindings: recentSources)
    return try ZoenMediaFiles.json(["accepted_revision": saved.acceptedRevision,
      "draft": ZoenMediaDraftBridge.object(saved.draft)])
  }

  func materializeMediaDraftAsset(_ request: String) throws -> String {
    let value = try ZoenMediaDraftBridge.decode(request)
    let core = try mediaDraftCore(value.scope)
    guard let revision = value.expectedRevision, let assetId = value.assetId else { throw ZoenMediaFiles.Failure.invalidFile }
    let draft = try core.mediaDraft(scope: value.scope.core)
    guard draft.revision == revision, let asset = draft.assets.first(where: { $0.id == assetId }) else { throw ZoenMediaFiles.Failure.invalidFile }
    let bytes = try core.mediaDraftAsset(scope: value.scope.core, expectedRevision: revision, assetId: assetId)
    let uri = try mediaFiles.stage(bytes, name: asset.name)
    var result = ZoenMediaDraftBridge.object(asset)
    result["uri"] = uri
    return try ZoenMediaFiles.json(result)
  }

  func previewMediaDraftAsset(_ request: String, presentationId: String, presenter: UIViewController?) async throws {
    guard let token = UUID(uuidString: presentationId), draftPreviewTasks[token] == nil else {
      throw ZoenMediaFiles.Failure.invalidFile
    }
    if let canceled = cancelledDraftPreviews.firstIndex(of: token) {
      cancelledDraftPreviews.remove(at: canceled)
      throw CancellationError()
    }
    let value = try ZoenMediaDraftBridge.decode(request)
    let core = try mediaDraftCore(value.scope)
    guard let revision = value.expectedRevision, let assetId = value.assetId else {
      throw ZoenMediaFiles.Failure.invalidFile
    }
    let draft = try core.mediaDraft(scope: value.scope.core)
    let content = try ZoenMediaDraftBridge.content(draft.contentJson)
    guard draft.revision == revision, content?["kind"] as? String == "attachments",
          let entries = content?["items"] as? [[String: Any]],
          entries.contains(where: { $0["asset_id"] as? String == assetId }),
          let asset = draft.assets.first(where: { $0.id == assetId }) else {
      throw ZoenMediaFiles.Failure.invalidFile
    }
    let bytes = try core.mediaDraftAsset(scope: value.scope.core, expectedRevision: revision, assetId: assetId)
    let uri = try mediaFiles.stage(bytes, name: asset.name)
    let file: ZoenPreviewFile
    do { file = try mediaFiles.takeForPreview(uri) }
    catch { mediaFiles.release(uri); throw error }
    // Transfer ownership BEFORE hopping to MainActor. Session erasure owns this
    // copy even if cancellation wins before UIKit can present it.
    let id = filePreview.register(file: file, generation: generation)
    let preview = filePreview
    let title = asset.name
    let mime = asset.mime
    let task = Task { @MainActor in
      try await preview.open(id: id, title: title, edit: nil) { editing in
        if editing == nil, ZoenMediaGalleryPreview.supports(mime: mime) {
          return ZoenMediaGalleryPreview(presenter: presenter, mime: mime)
        }
        return ZoenQuickLookPreview(presenter: presenter, editing: editing)
      }
    }
    draftPreviewTasks[token] = task
    // No core work follows this await. The existing preview owner, rather than
    // requestDrain, lets close/erasure dismiss a read-only presentation.
    defer { draftPreviewTasks.removeValue(forKey: token) }
    try await task.value
  }

}
