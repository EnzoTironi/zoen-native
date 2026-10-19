import Foundation

/// Typed Rust boundary shared by the app session and native integration tests.
/// Account/item provenance is pinned by ZoenSession before constructing this adapter.
enum ZoenPDFEditCore {
  static func make(engine core: Client, scope: FileEditDraftScope,
                   observedBase: ZoenFilePreview.EditBase, maximumBytes: UInt64) throws -> ZoenPDFEditJournal {
    let backend = ZoenPDFEditJournal.Backend(
      load: { try Self.pdfOperation { try Self.pdfDraftState(core.fileEditDraft(scope: scope)) } },
      stage: { request in
        try Self.pdfOperation {
          let result = try core.stageFileEditDraft(scope: scope, expectedRevision: request.expectedRevision,
            editorId: request.editorID, captureId: request.captureID,
            originVersion: request.origin.version, originSha256: request.origin.sha256, bytes: request.bytes)
          return ZoenPDFEditJournal.Acknowledgment(acceptedRevision: result.acceptedRevision,
            draft: try Self.pdfDraftState(result.draft))
        }
      },
      bytes: { revision, capture in
        try Self.pdfOperation { try core.fileEditDraftBytes(scope: scope, expectedRevision: revision, captureId: capture) }
      },
      commit: { editor, capture in
        try Self.pdfOperation {
          let result = try core.commitFileEditDraft(scope: scope, editorId: editor, captureId: capture,
            note: NSLocalizedString("Marked up", comment: "PDF file version history note"))
          return ZoenFilePreview.EditBase(version: result.version, sha256: result.sha256)
        }
      },
      discard: { revision in
        try Self.pdfOperation { try Self.pdfDraftState(core.discardFileEditDraft(scope: scope, expectedRevision: revision)) }
      })
    return try ZoenPDFEditJournal(observedBase: observedBase, maximumBytes: maximumBytes, backend: backend)
  }

  private static func pdfDraftState(_ draft: FileEditDraftDto) throws -> ZoenPDFEditJournal.State {
    func base(_ version: UInt32?, _ sha256: String?) throws -> ZoenFilePreview.EditBase? {
      if version == nil && sha256 == nil { return nil }
      guard let version, let sha256 else { throw ZoenFilePreview.Failure.changed }
      return .init(version: version, sha256: sha256)
    }
    return try .init(revision: draft.revision, editorID: draft.editorId,
      origin: base(draft.originVersion, draft.originSha256), base: base(draft.baseVersion, draft.baseSha256),
      pending: draft.pending.map { .init(id: $0.captureId, sha256: $0.sha256, size: $0.size) })
  }

  private static func pdfOperation<T>(_ operation: () throws -> T) throws -> T {
    do { return try operation() }
    catch let error as CoreError {
      let message: String
      switch error {
      case .Storage(let reason), .Forbidden(let reason), .Invalid(let reason), .Stale(let reason), .DraftRejected(let reason): message = reason
      case .NotFound: message = NSLocalizedString("This file version is unavailable.", comment: "Edited PDF was removed")
      }
      throw NSError(domain: "Zoen.FileEdit", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
    }
  }

}
