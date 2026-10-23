import Foundation

@MainActor extension ZoenFilePreview {
  func capture(_ result: Result<ZoenPreviewFile, ZoenPDFEditCapture.Failure>, session: Session) {
    switch result {
    case .success(let file):
      guard active === session, session.forced == nil, !session.discardRequested else {
        do { try file.remove() } catch { retained.append(file) }
        return
      }
      session.pending.append(Session.Edit(file: file))
      session.captureFailure = nil
      do {
        if let retain = session.edit?.retain {
          try scope.admit(session.generation) { try retain(file) }
        }
      } catch {
        // Retain the owned copy for explicit retry. The save adapter can retry
        // private ingestion before publishing; this is not a durable ACK.
        session.failure = error
      }
      if session.failure == nil { startWorker(session) }
      else { completeEditing(session) }
    case .failure(let failure):
      guard active === session, session.forced == nil, !session.discardRequested else {
        if let file = failure.retained { retained.append(file) }
        return
      }
      if let file = failure.retained { session.failedCaptureCopies.append(file) }
      let error = session.mediaEdit == nil ? Failure.editedPDF : Failure.editedMedia
      session.captureFailure = error
      session.presentation.captureFailed(error: error)
      completeEditing(session)
    }
  }

  private func admitVideoCapture(_ session: Session) throws {
    try Task.checkCancellation()
    guard active === session, !session.stopping, !session.viewerGone,
          session.forced == nil, !session.discardRequested,
          !session.keepRequested, session.mediaEdit?.kind == .video else { throw Failure.changed }
    try scope.admit(session.generation) {}
  }

  func captureVideo(_ render: @escaping @Sendable (URL, UInt64) async throws -> Void,
                            session: Session) async throws {
    try admitVideoCapture(session)
    guard session.renderTask == nil, session.worker == nil, let context = session.mediaEdit else { throw Failure.busy }
    let file: ZoenPreviewFile
    switch ZoenMediaEditCapture.allocateVideo() {
    case .success(let value): file = value
    case .failure(let failure):
      capture(.failure(failure), session: session)
      throw failure.cause
    }
    // This is registered before the first suspension/export. Forced close owns
    // every partial byte and must drain rendering before deleting the input.
    session.renderFile = file
    let task = Task { @MainActor [self, session] in
      defer {
        session.renderTask = nil; session.renderFile = nil
        completeEditing(session)
      }
      do {
        try await context.authorize()
        try admitVideoCapture(session)
        try await render(file.url, context.maximumBytes)
        try admitVideoCapture(session)
        try await context.authorize()
        try admitVideoCapture(session)
        _ = try await ZoenMediaEditCapture.sealVideo(file, maximumBytes: context.maximumBytes)
        try admitVideoCapture(session)
        try await context.authorize()
        try admitVideoCapture(session)
        capture(.success(file), session: session)
      } catch {
        do { try file.remove() }
        catch {
          // Deletion failure remains attached to this owner until actual
          // dismissal/clear retries it; cancellation never loses the lease.
          session.failedCaptureCopies.append(file)
        }
        throw error
      }
    }
    session.renderTask = task
    try await withTaskCancellationHandler { try await task.value } onCancel: { task.cancel() }
  }

  private func startWorker(_ session: Session) {
    guard active === session, session.worker == nil, session.failure == nil,
          session.forced == nil, !session.discardRequested, !session.keepRequested,
          !session.pending.isEmpty, session.editable else { return }
    session.worker = Task { @MainActor [self, session] in
      while !session.pending.isEmpty, session.forced == nil, session.failure == nil,
            !session.discardRequested, !session.keepRequested {
        do {
          if !session.pending[0].accepted {
            if let media = session.mediaEdit {
              try await media.save(session.pending[0].file)
            } else if let context = session.edit, let base = session.base {
              session.base = try await context.save(session.pending[0].file, base)
            } else { throw Failure.changed }
            session.pending[0].accepted = true
          }
          try session.pending[0].file.remove()
          session.pending.removeFirst()
        } catch {
          session.failure = error
          break
        }
      }
      session.worker = nil
      completeEditing(session)
    }
  }

  private func startDiscard(_ session: Session) {
    guard active === session, session.discardRequested, session.worker == nil,
          session.forced == nil, session.editable else { return }
    session.worker = Task { @MainActor [self, session] in
      do {
        // Keep every plaintext lease until the canonical private discard ACK.
        // A failed/ambiguous discard can only retry this operation, never save.
        try await session.edit?.discard?()
        if session.forced == nil {
          for edit in session.pending {
            do { try edit.file.remove() } catch { retained.append(edit.file); session.cleanupFailure = error }
          }
          session.pending.removeAll()
          session.failure = nil
          session.captureFailure = nil
        }
      } catch { session.failure = error }
      session.worker = nil
      completeEditing(session)
    }
  }

  private func keepForLater(_ session: Session) {
    guard active === session, session.worker == nil, session.forced == nil,
          !session.discardRequested,
          session.pending.contains(where: { !$0.accepted }), let retain = session.edit?.retain else { return }
    // A newer unreadable callback cannot be acknowledged as privately retained.
    guard session.captureFailure == nil else { completeEditing(session); return }
    session.keepRequested = true
    do {
      // These copies were admitted before ordinary close retired presentation.
      // Reconcile their exact private ACKs without admitting any new capture.
      for edit in session.pending where !edit.accepted { try retain(edit.file) }
      finish(session, error: nil)
    } catch {
      session.failure = error
      completeEditing(session)
    }
  }

  func viewerEnded(_ session: Session, error: Error?) {
    guard active === session else { return }
    if !session.editable { finish(session, error: error); return }
    session.viewerGone = true
    if let error, session.failure == nil { session.failure = error }
    completeEditing(session)
  }

  func completeEditing(_ session: Session) {
    guard active === session, session.editable, session.viewerGone,
          session.worker == nil, session.renderTask == nil else { return }
    if let forced = session.forced {
      for edit in session.pending {
        do { try edit.file.remove() } catch { retained.append(edit.file); session.cleanupFailure = error }
      }
      session.pending.removeAll()
      finish(session, error: forced)
    } else if let failure = session.failure ?? session.captureFailure {
      guard !session.recoveryShown else { return }
      session.recoveryShown = true
      let retry: (@MainActor () -> Void)?
      if (session.pending.isEmpty && !session.discardRequested)
        || (session.keepRequested && session.captureFailure != nil) { retry = nil }
      else {
        retry = { @MainActor [weak self, weak session] in
          guard let self, let session, self.active === session, session.forced == nil,
                session.recoveryShown, session.worker == nil else { return }
          session.recoveryShown = false
          session.failure = nil
          if session.discardRequested { self.startDiscard(session) }
          else if session.keepRequested { self.keepForLater(session) }
          else { self.startWorker(session) }
        }
      }
      let changesSaved = !session.pending.isEmpty && session.pending.allSatisfy(\.accepted) && session.captureFailure == nil
      let keep: (@MainActor () -> Void)?
      if session.edit?.retain != nil, session.pending.contains(where: { !$0.accepted }),
         session.captureFailure == nil, !session.discardRequested {
        keep = { @MainActor [weak self, weak session] in
          guard let self, let session, self.active === session, session.forced == nil,
                session.recoveryShown, session.worker == nil else { return }
          session.recoveryShown = false
          session.failure = nil
          self.keepForLater(session)
        }
      } else { keep = nil }
      session.presentation.recover(error: changesSaved ? Failure.cleanup : failure, changesSaved: changesSaved, retry: retry, keep: keep) { [weak self, weak session] in
        guard let self, let session, self.active === session, session.forced == nil,
              session.recoveryShown, session.worker == nil else { return }
        session.discardRequested = true
        session.keepRequested = false
        session.recoveryShown = false
        session.failure = nil
        self.startDiscard(session)
      }
    } else if session.pending.isEmpty { finish(session, error: nil) }
  }

}
