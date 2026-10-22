import ExpoModulesCore
import Foundation
import UIKit
import PDFKit
#if canImport(FoundationModels)
import FoundationModels
#endif

extension ZoenSession {
  func previewLocalFile(_ uri: String, presenter: UIViewController?) async throws {
    let identity = try mediaFiles.ownedSelectionURL(uri).path
    guard !erasing, !closing, let binding = previewBindings[identity], bindingIsCurrent(binding) else { throw SessionError.closed }
    // Historical versions stay read-only. Only ordinary Open of the current PDF
    // discovers this device's private edits; it never changes the shared version.
    let journal: ZoenPDFEditJournal?
    if mode == "account", binding.mime == "application/pdf",
       try binding.engine.item(itemId: binding.itemId).version == binding.version {
      journal = try pdfEditJournal(binding)
    } else { journal = nil }
    let recovering = journal?.recovered.pending.isEmpty == false
    let canEdit = binding.mime == "application/pdf"
      ? try binding.engine.fileEditAllowed(itemId: binding.itemId, expectedVersion: binding.version, expectedSha256: binding.sha256)
      : false
    var retain: (@Sendable (ZoenPreviewFile) throws -> Void)?
    var discard: (@Sendable () async throws -> Void)?
    if let journal {
      retain = { file in try journal.retain(file) }
      discard = { try journal.discard() }
    }
    let edit: ZoenFilePreview.EditContext?
    if recovering || canEdit {
      edit = ZoenFilePreview.EditContext(
        base: journal?.initialBase ?? .init(version: binding.version, sha256: binding.sha256),
        maximumBytes: ZoenMediaFiles.maximumBytes,
        canCapture: canEdit,
        retain: retain,
        discard: discard,
        save: { [weak self] file, base in
          guard let self else { throw SessionError.closed }
          return try await self.savePreviewEdit(file, base: base, binding: binding, journal: journal)
        })
    } else { edit = nil }
    var recovered: [ZoenPreviewFile] = []
    var viewer: ZoenPreviewFile?
    do {
      if recovering, let journal {
        // Each FIFO capture and the viewer have independent leases. Committing a
        // queued snapshot cannot remove the PDF still being read by Quick Look.
        for capture in journal.recovered.pending {
          let bytes = try journal.recoveredBytes(capture)
          let staged = try mediaFiles.stage(bytes, name: binding.name)
          let file = try mediaFiles.takeForPreview(staged)
          recovered.append(file)
          try journal.registerRecovered(file, capture: capture)
          if capture == journal.recovered.pending.last {
            let stagedViewer = try mediaFiles.stage(bytes, name: binding.name)
            viewer = try mediaFiles.takeForPreview(stagedViewer)
          }
        }
      } else { viewer = try mediaFiles.takeForPreview(uri) }
    } catch {
      let owned = recovered + (viewer.map { [$0] } ?? [])
      if let first = owned.first {
        let ticket = filePreview.register(file: first, generation: generation, recovered: Array(owned.dropFirst()))
        try await filePreview.abandon(id: ticket)
      }
      throw error
    }
    guard let file = viewer else { throw ZoenMediaFiles.Failure.invalidFile }
    previewBindings.removeValue(forKey: identity)
    // Recovery may open a separate viewer while the original remains owned.
    // Keep that original's source provenance until its actual lease is retired.
    if mediaFiles.releaseIdentity(uri) == nil { recentSelections.removeValue(forKey: identity) }
    let id = filePreview.register(file: file, generation: generation, recovered: recovered)
    let title = recovering
      ? "\(binding.name) — \(NSLocalizedString("Recovered changes", comment: "Recovered private PDF preview title"))"
      : binding.name
    try await filePreview.open(id: id, title: title, edit: edit) { editing in
      if editing == nil, ZoenMediaGalleryPreview.supports(mime: binding.mime) {
        return ZoenMediaGalleryPreview(presenter: presenter, mime: binding.mime)
      }
      return ZoenQuickLookPreview(presenter: presenter, editing: editing)
    }
  }

  private func pdfEditJournal(_ binding: PreviewBinding) throws -> ZoenPDFEditJournal {
    guard let owner = binding.scope, bindingIsCurrent(binding), binding.mode == "account" else { throw SessionError.closed }
    let scope = FileEditDraftScope(expectedIdentityId: owner.identity, expectedDeviceId: owner.device,
      spaceId: binding.spaceId, itemId: binding.itemId)
    return try ZoenPDFEditCore.make(engine: binding.engine, scope: scope,
      observedBase: .init(version: binding.version, sha256: binding.sha256), maximumBytes: ZoenMediaFiles.maximumBytes)
  }

  private func savePreviewEdit(_ file: ZoenPreviewFile, base: ZoenFilePreview.EditBase,
                               binding: PreviewBinding, journal: ZoenPDFEditJournal?) throws -> ZoenFilePreview.EditBase {
    // An admitted edit may finish while plain close waits for this viewer. Erase
    // and account replacement still fence it; the checked core mutation owns the
    // definitive version/hash/access comparison and append in one transaction.
    guard bindingIsCurrent(binding), binding.mime == "application/pdf" else { throw SessionError.closed }
    if let journal {
      let saved = try journal.save(file)
      listener?.onChange(spaceIds: [binding.spaceId])
      return saved
    }
    let selection = try mediaFiles.readCapturedSelection(file,
      name: binding.name, mime: "application/pdf")
    guard selection.data.starts(with: Data("%PDF-".utf8)),
          let document = PDFDocument(data: selection.data), !document.isLocked,
          document.pageCount > 0 else { throw ZoenMediaFiles.Failure.invalidFile }
    let saved: ItemDetail
    do {
      saved = try binding.engine.fileNewVersionChecked(itemId: binding.itemId,
        expectedVersion: base.version, expectedSha256: base.sha256,
        bytes: selection.data, thumbnail: nil,
        note: NSLocalizedString("Marked up", comment: "PDF file version history note"))
    } catch let error as CoreError {
      let message: String
      switch error {
      case .Storage(let reason), .Forbidden(let reason), .Invalid(let reason), .Stale(let reason), .DraftRejected(let reason): message = reason
      case .NotFound: message = NSLocalizedString("This file version is unavailable.", comment: "Edited PDF was removed")
      }
      throw NSError(domain: "Zoen.FileEdit", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
    }
    guard let file = saved.file else { throw ZoenMediaFiles.Failure.invalidFile }
    listener?.onChange(spaceIds: [binding.spaceId])
    return .init(version: saved.version, sha256: file.sha256)
  }

  func saveProfile(_ request: String) throws {
    guard !erasing, !closing, request.utf8.count <= 8192, let core else { throw SessionError.closed }
    let save = try JSONDecoder().decode(ZoenProfilePhotos.Save.self, from: Data(request.utf8))
    guard let account = core.account(), account.unlocked, !account.setupPending, account.identityId == save.ownerId, account.deviceId == save.deviceId else { throw ZoenProfilePhotos.Failure.changedSession }
    _ = try core.updateMyProfile(name: save.name, bio: save.bio, photo: profilePhotos.change(save))
    listener?.onChange(spaceIds: [])
  }

  func materializeProfilePhoto(ownerId: String, deviceId: String, identityId: String, expectedSha256: String) throws -> String {
    guard !erasing, !closing, let core, let account = core.account(), account.unlocked, !account.setupPending, account.identityId == ownerId, account.deviceId == deviceId else { throw ZoenProfilePhotos.Failure.changedSession }
    guard let photo = try core.profilePhoto(identityId: identityId, expectedSha256: expectedSha256) else { return "null" }
    return try profilePhotos.stage(photo)
  }

  func releaseProfilePhoto(_ uri: String) { profilePhotos.release(uri) }

}
