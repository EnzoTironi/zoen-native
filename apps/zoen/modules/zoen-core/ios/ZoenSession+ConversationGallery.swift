import ExpoModulesCore
import Foundation
import UIKit
import PDFKit
#if canImport(FoundationModels)
import FoundationModels
#endif

extension ZoenSession {
  struct ConversationGalleryScope: Sendable {
    let engine: Client
    let generation: UInt64
    let mode: String
    let owner: FileScope
    let spaceId: String
    let gate: ZoenMediaEditGate
  }
  private func checkConversationGallery(_ scope: ConversationGalleryScope) throws {
    try Task.checkCancellation(); try scope.gate.admitted { () }
    guard !erasing, !closing, self.core === scope.engine, generation == scope.generation,
          mode == scope.mode, try fileScope(spaceId: scope.spaceId, engine: scope.engine) == scope.owner else {
      throw ZoenFilePreview.Failure.changed
    }
  }
  private func conversationGalleryMessage(_ reference: ZoenConversationGalleryRequest.Reference,
                                         scope: ConversationGalleryScope) async throws -> ZoenConversationGalleryRequest.Message {
    try checkConversationGallery(scope)
    let request = try ZoenMediaFiles.json(["op": "message", "space_id": scope.spaceId, "message_id": reference.messageId])
    let result = try await scope.engine.clientRequest(request: request)
    try checkConversationGallery(scope)
    let message = try ZoenConversationGalleryRequest.Message(json: result, expected: reference)
    guard try scope.engine.item(itemId: reference.itemId).spaceId == scope.spaceId else { throw ZoenFilePreview.Failure.changed }
    return message
  }

  func previewConversationMedia(_ request: String, presentationId: String, presenter: UIViewController?) async throws {
    guard let token = UUID(uuidString: presentationId), draftPreviewTasks.isEmpty else { throw ZoenFilePreview.Failure.busy }
    if let index = cancelledDraftPreviews.firstIndex(of: token) {
      cancelledDraftPreviews.remove(at: index); throw CancellationError()
    }
    let gate = ZoenMediaEditGate()
    try draftMediaEditLifetime.register(token, gate: gate)
    let task = Task { try await self.openConversationMedia(request, token: token, presenter: presenter, gate: gate) }
    draftPreviewTasks[token] = task
    defer { conversationGallerySnapshots.removeValue(forKey: token); draftPreviewTasks.removeValue(forKey: token); draftMediaEditLifetime.remove(token) }
    try await task.value
  }

  private func openConversationMedia(_ json: String, token: UUID, presenter: UIViewController?, gate: ZoenMediaEditGate) async throws {
    let request = try ZoenConversationGalleryRequest.decode(json)
    guard !closing, !erasing, let engine = core else { throw SessionError.closed }
    let scope = ConversationGalleryScope(engine: engine, generation: generation, mode: mode,
      owner: try fileScope(spaceId: request.spaceId, engine: engine), spaceId: request.spaceId, gate: gate)
    var candidates: [ZoenConversationGalleryRequest.Message] = []
    for reference in request.entries { candidates.append(try await conversationGalleryMessage(reference, scope: scope)) }
    let messages = try request.reconstruct(candidates)
    let items = try messages.map { message -> ZoenConversationMediaGallery.Item in
      let reference = message.reference
      let info = try engine.fileInfoAt(itemId: reference.itemId, version: reference.version)
      return .init(message: message, name: info.name, mime: info.mime, sha256: info.sha256)
    }
    guard let selected = items.first(where: { $0.message.reference.messageId == request.selectedId }) else { throw ZoenFilePreview.Failure.changed }
    conversationGallerySnapshots[token] = (scope, items)
    // Unsupported visual formats retain the exact authorized single-file viewer.
    if !items.allSatisfy({ ZoenMediaGalleryPreview.supports(mime: $0.mime) }) {
      let local = try await materializeFile(itemId: selected.message.reference.itemId, version: Int(selected.message.reference.version))
      guard let value = try JSONSerialization.jsonObject(with: Data(local.utf8)) as? [String: Any],
            let uri = value["uri"] as? String else { throw ZoenFilePreview.Failure.changed }
      do {
        try checkConversationGallery(scope)
        guard try await conversationGalleryMessage(selected.message.reference, scope: scope) == selected.message else { throw ZoenFilePreview.Failure.changed }
        try await previewLocalFile(uri, presenter: presenter)
      } catch { releaseLocalFile(uri); throw error }
      return
    }
    let copies = await ZoenFilePreview.ReadOnlyCopies()
    let ticket = try await conversationGalleryCopy(selected, scope: scope, copyOwner: copies.registration)
    let preview = filePreview
    do {
      try checkConversationGallery(scope)
      try await MainActor.run { try scope.gate.admitted { () } }
    } catch { try await preview.abandon(id: ticket); throw error }
    try await preview.open(id: ticket, title: selected.name, edit: nil, readOnlyCopies: copies,
      initialKey: request.selectedId) { [weak self] _ in
      ZoenConversationMediaGallery(presenter: presenter, items: items, selectedId: request.selectedId, copies: copies) { item in
        guard let self else { throw ZoenFilePreview.Failure.changed }
        return try await self.conversationGalleryCopy(item, scope: scope, copyOwner: copies.registration)
      }
    }
  }

  private func conversationGalleryCopy(_ item: ZoenConversationMediaGallery.Item,
                                       scope: ConversationGalleryScope,
                                       copyOwner: ZoenFilePreview.ReadOnlyRegistration) async throws -> UUID {
    let reference = item.message.reference
    guard try await conversationGalleryMessage(reference, scope: scope) == item.message else { throw ZoenFilePreview.Failure.changed }
    var info = try scope.engine.fileInfoAt(itemId: reference.itemId, version: reference.version)
    guard info.sha256 == item.sha256, info.name == item.name, info.mime == item.mime else { throw ZoenFilePreview.Failure.changed }
    let local = try await materializeFile(itemId: reference.itemId, version: Int(reference.version))
    guard let value = try JSONSerialization.jsonObject(with: Data(local.utf8)) as? [String: Any],
          let uri = value["uri"] as? String else { throw ZoenFilePreview.Failure.changed }
    do {
      try checkConversationGallery(scope)
      guard try await conversationGalleryMessage(reference, scope: scope) == item.message else { throw ZoenFilePreview.Failure.changed }
      info = try scope.engine.fileInfoAt(itemId: reference.itemId, version: reference.version)
      guard info.sha256 == item.sha256, info.name == item.name, info.mime == item.mime else { throw ZoenFilePreview.Failure.changed }
      let identity = try mediaFiles.ownedSelectionURL(uri).path
      guard let binding = previewBindings[identity], bindingIsCurrent(binding), binding.scope == scope.owner,
            binding.itemId == reference.itemId, binding.version == reference.version, binding.sha256 == item.sha256 else { throw ZoenFilePreview.Failure.changed }
      let file = try mediaFiles.takeForPreview(uri)
      previewBindings.removeValue(forKey: identity); recentSelections.removeValue(forKey: identity)
      return try filePreview.registerReadOnly(file: file, generation: scope.generation, owner: copyOwner)
    } catch { releaseLocalFile(uri); throw error }
  }

  /// Native source changes revalidate the ephemeral request even if JavaScript
  /// is stalled. Coalescing bounds work to one validation loop for one viewer.
  func validateConversationGalleryChanges(_ spaces: [String]) {
    guard conversationGallerySnapshots.values.contains(where: { spaces.isEmpty || spaces.contains($0.0.spaceId) }) else { return }
    if galleryValidation != nil { galleryValidationAgain = true; return }
    galleryValidation = Task { [weak self] in
      guard let self else { return }
      await self.drainConversationGalleryValidation()
    }
  }
  private func drainConversationGalleryValidation() async {
    defer { galleryValidation = nil }
    repeat {
      galleryValidationAgain = false
      for (token, snapshot) in conversationGallerySnapshots {
        do {
          for item in snapshot.1 {
            guard conversationGallerySnapshots[token] != nil else { break }
            guard try await conversationGalleryMessage(item.message.reference, scope: snapshot.0) == item.message else { throw ZoenFilePreview.Failure.changed }
            let info = try snapshot.0.engine.fileInfoAt(itemId: item.message.reference.itemId, version: item.message.reference.version)
            guard info.sha256 == item.sha256, info.name == item.name, info.mime == item.mime else { throw ZoenFilePreview.Failure.changed }
          }
        } catch {
          draftMediaEditLifetime.cancel(token)
          draftPreviewTasks[token]?.cancel()
        }
      }
    } while galleryValidationAgain && !conversationGallerySnapshots.isEmpty
  }

  func releaseLocalFile(_ uri: String) {
    if let identity = mediaFiles.releaseIdentity(uri) {
      recentSelections.removeValue(forKey: identity)
      previewBindings.removeValue(forKey: identity)
    }
    mediaFiles.release(uri)
  }

}
