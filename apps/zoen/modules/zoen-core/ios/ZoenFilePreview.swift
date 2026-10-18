import Foundation

/// One exact directory transferred from ZoenMediaFiles, never an arbitrary JS URL.
struct ZoenPreviewFile: Sendable {
  let url: URL
  let directory: URL

  static func validated(url: URL, directory: URL, cacheRoot: URL, maximumBytes: UInt64) throws -> Self {
    let manager = FileManager.default
    guard url.isFileURL, directory.isFileURL, cacheRoot.isFileURL,
          UUID(uuidString: directory.lastPathComponent) != nil,
          url.deletingLastPathComponent().standardizedFileURL == directory.standardizedFileURL else {
      throw ZoenFilePreview.Failure.unavailable
    }
    // iOS may spell an OS-owned cache ancestor as /var or /private/var. Resolve
    // those aliases, but never follow a substituted preview directory or file.
    let folderAttributes = try manager.attributesOfItem(atPath: directory.path)
    let fileAttributes = try manager.attributesOfItem(atPath: url.path)
    let folder = directory.resolvingSymlinksInPath().standardizedFileURL
    let file = url.resolvingSymlinksInPath().standardizedFileURL
    let namespace = cacheRoot.resolvingSymlinksInPath().standardizedFileURL.appendingPathComponent("ZoenMedia", isDirectory: true)
    guard folderAttributes[.type] as? FileAttributeType == .typeDirectory,
          fileAttributes[.type] as? FileAttributeType == .typeRegular,
          folder.deletingLastPathComponent() == namespace,
          file.deletingLastPathComponent() == folder,
          let size = fileAttributes[.size] as? NSNumber, size.uint64Value > 0,
          size.uint64Value <= maximumBytes else { throw ZoenFilePreview.Failure.unavailable }
    return Self(url: file, directory: folder)
  }

  func remove() throws {
    do { try FileManager.default.removeItem(at: directory) }
    catch let error as CocoaError where error.code == .fileNoSuchFile || error.code == .fileReadNoSuchFile { return }
    // An inaccessible parent is an error, not evidence that a copy was deleted.
    do {
      _ = try FileManager.default.attributesOfItem(atPath: directory.path)
      throw ZoenFilePreview.Failure.cleanup
    } catch let error as CocoaError where error.code == .fileNoSuchFile || error.code == .fileReadNoSuchFile {}
  }
}

@MainActor protocol ZoenFilePreviewPresentation: AnyObject {
  func show(file: URL, title: String, ended: @escaping @MainActor (Error?) -> Void) throws
  /// Completion covers the viewer and any activity/share controller it presented.
  func dismiss(completion: @escaping @MainActor () -> Void)
  func recover(error: Error, changesSaved: Bool, retry: (@MainActor () -> Void)?, discard: @escaping @MainActor () -> Void)
  func captureFailed(error: Error)
}

extension ZoenFilePreviewPresentation {
  func recover(error: Error, changesSaved: Bool, retry: (@MainActor () -> Void)?, discard: @escaping @MainActor () -> Void) {}
  func captureFailed(error: Error) {}
}

/// The main-actor owner is independent of the message row and Expo's presentation
/// Promise. Its continuation ends only after actual dismissal and copy deletion.
@MainActor final class ZoenFilePreview {
  enum Failure: Error, LocalizedError {
    case changed, busy, unavailable, cleanup, editedPDF
    var errorDescription: String? {
      switch self {
      case .changed: NSLocalizedString("This preview is no longer available.", comment: "Native preview session changed")
      case .busy: NSLocalizedString("Close the current preview and try again.", comment: "Native preview is already open")
      case .unavailable: NSLocalizedString("This file could not be previewed. Try again.", comment: "Native file preview unavailable")
      case .cleanup: NSLocalizedString("The temporary preview could not be removed. Try again.", comment: "Native preview cleanup failed")
      case .editedPDF: NSLocalizedString("The edited PDF could not be read. Try saving your changes again.", comment: "Native PDF edit could not be retained")
      }
    }
  }

  struct EditBase: Sendable, Equatable {
    let version: UInt32
    let sha256: String
  }

  struct EditContext: Sendable {
    let base: EditBase
    let maximumBytes: UInt64
    let save: @Sendable (ZoenPreviewFile, EditBase) async throws -> EditBase
  }

  @MainActor final class Editing {
    private let maximumBytes: UInt64
    fileprivate var captured: ((Result<ZoenPreviewFile, ZoenPDFEditCapture.Failure>) -> Void)?
    fileprivate init(maximumBytes: UInt64) { self.maximumBytes = maximumBytes }
    func receive(_ url: URL) { captured?(ZoenPDFEditCapture.stage(url, maximumBytes: maximumBytes)) }
  }

  private final class Scope: @unchecked Sendable {
    struct Transfer: Sendable {
      let generation: UInt64
      let file: ZoenPreviewFile
    }
    private let lock = NSLock()
    private var floor: UInt64 = 0
    private var pending: [UUID: Transfer] = [:]
    func register(file: ZoenPreviewFile, generation: UInt64) -> UUID {
      lock.lock(); defer { lock.unlock() }
      let id = UUID()
      pending[id] = Transfer(generation: generation, file: file)
      return id
    }
    func take(_ id: UUID) -> Transfer? {
      lock.lock(); defer { lock.unlock() }
      return pending.removeValue(forKey: id)
    }
    func drain(before generation: UInt64) -> [Transfer] {
      lock.lock(); defer { lock.unlock() }
      let ids = pending.keys.filter { pending[$0]!.generation < generation }
      return ids.compactMap { pending.removeValue(forKey: $0) }
    }
    func invalidate(before generation: UInt64) {
      lock.lock(); defer { lock.unlock() }
      floor = max(floor, generation)
    }
    func admit(_ generation: UInt64, show: () throws -> Void) throws {
      lock.lock(); defer { lock.unlock() }
      guard generation >= floor else { throw Failure.changed }
      // Invalidation and UIKit's synchronous admission have one ordering. A
      // cleanup that wins this gate prevents show; otherwise it dismisses show.
      try show()
    }
  }

  private final class Session {
    struct Edit {
      let file: ZoenPreviewFile
      var accepted = false
    }
    let id: UUID
    let generation: UInt64
    let file: ZoenPreviewFile
    let presentation: any ZoenFilePreviewPresentation
    let continuation: CheckedContinuation<Void, Error>
    var stopping = false
    var waiters: [CheckedContinuation<Void, Error>] = []
    var lifecycle: [CheckedContinuation<Void, Error>] = []
    let edit: EditContext?
    let editing: Editing?
    var base: EditBase?
    var pending: [Edit] = []
    var worker: Task<Void, Never>?
    var failure: Error?
    var captureFailure: Error?
    var cleanupFailure: Error?
    var failedCaptureCopies: [ZoenPreviewFile] = []
    var recoveryShown = false
    var viewerGone = false
    var forced: Error?
    init(id: UUID, generation: UInt64, file: ZoenPreviewFile, presentation: any ZoenFilePreviewPresentation,
         continuation: CheckedContinuation<Void, Error>, edit: EditContext?, editing: Editing?) {
      self.id = id; self.generation = generation; self.file = file; self.presentation = presentation
      self.continuation = continuation
      self.edit = edit; self.editing = editing; self.base = edit?.base
    }
  }

  private nonisolated let scope = Scope()
  private var active: Session?
  private var retained: [ZoenPreviewFile] = []
  nonisolated init() {}

  /// Synchronous invalidation closes the actor-to-main-actor scheduling gap: an
  /// old queued presentation cannot begin after account cleanup has started.
  nonisolated func invalidate(before generation: UInt64) { scope.invalidate(before: generation) }

  /// Register on ZoenSession BEFORE leaving its actor. Cleanup can therefore own
  /// a transferred file even if its queued main-actor presentation never starts.
  nonisolated func register(file: ZoenPreviewFile, generation: UInt64) -> UUID {
    scope.register(file: file, generation: generation)
  }

  func open(id: UUID, title: String,
            makePresentation: @MainActor () throws -> any ZoenFilePreviewPresentation) async throws {
    try await open(id: id, title: title, edit: nil) { _ in try makePresentation() }
  }

  func open(id: UUID, title: String, edit: EditContext?,
            makePresentation: @MainActor (Editing?) throws -> any ZoenFilePreviewPresentation) async throws {
    guard let transfer = scope.take(id) else { throw Failure.changed }
    let file = transfer.file
    let generation = transfer.generation
    let presentation: any ZoenFilePreviewPresentation
    let editing = edit.map { Editing(maximumBytes: $0.maximumBytes) }
    do {
      guard !Task.isCancelled else { throw Failure.changed }
      guard active == nil, retained.isEmpty else { throw Failure.busy }
      guard !title.contains("\0"), title.utf8.count <= 1024 else { throw Failure.unavailable }
      presentation = try makePresentation(editing)
    } catch {
      do { try file.remove() } catch { retained.append(file); throw error }
      throw error
    }
    let id = UUID()
    try await withTaskCancellationHandler {
      try await withCheckedThrowingContinuation { continuation in
        let session = Session(id: id, generation: generation, file: file, presentation: presentation,
                              continuation: continuation, edit: edit, editing: editing)
        active = session
        editing?.captured = { [weak self, weak session] result in
          guard let self, let session else { return }
          self.capture(result, session: session)
        }
        // No suspension occurs between admission and show. UIKit receives only a
        // still-owned file, and any synchronous presentation failure ends it too.
        do {
          try scope.admit(generation) {
            try presentation.show(file: file.url, title: title) { [weak self, weak session] error in
              guard let self, let session, !session.stopping else { return }
              self.viewerEnded(session, error: error)
            }
          }
          if Task.isCancelled, edit == nil { stop(session, error: CancellationError()) }
        } catch { finish(session, error: error) }
      }
    } onCancel: {
      Task { @MainActor [weak self] in self?.cancel(id) }
    }
  }

  private func cancel(_ id: UUID) {
    guard let active, id == active.id else { return }
    // Row cancellation must not discard a native editor's unsaved changes.
    guard active.edit == nil else { return }
    stop(active, error: CancellationError())
  }

  /// Ordinary account changes wait for the editor and explicit error recovery.
  /// They must retain the old engine until all accepted edits have settled.
  func prepareForClose(before generation: UInt64) async throws {
    // Retire queued presentations in the same main-actor turn as the active
    // editor check. A registered open cannot slip in after this check returns.
    invalidate(before: generation)
    guard let active, active.edit != nil else { return }
    try await withCheckedThrowingContinuation { active.lifecycle.append($0) }
  }

  private func capture(_ result: Result<ZoenPreviewFile, ZoenPDFEditCapture.Failure>, session: Session) {
    switch result {
    case .success(let file):
      guard active === session, session.forced == nil else {
        do { try file.remove() } catch { retained.append(file) }
        return
      }
      session.pending.append(Session.Edit(file: file))
      session.captureFailure = nil
      if session.failure == nil { startWorker(session) }
    case .failure(let failure):
      guard active === session, session.forced == nil else {
        if let file = failure.retained { retained.append(file) }
        return
      }
      if let file = failure.retained { session.failedCaptureCopies.append(file) }
      session.captureFailure = Failure.editedPDF
      session.presentation.captureFailed(error: Failure.editedPDF)
      completeEditing(session)
    }
  }

  private func startWorker(_ session: Session) {
    guard active === session, session.worker == nil, session.failure == nil,
          session.forced == nil, !session.pending.isEmpty, let context = session.edit else { return }
    session.worker = Task { @MainActor [self, session] in
      while !session.pending.isEmpty, session.forced == nil {
        do {
          if !session.pending[0].accepted {
            guard let base = session.base else { throw Failure.changed }
            let accepted = try await context.save(session.pending[0].file, base)
            session.base = accepted
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

  private func viewerEnded(_ session: Session, error: Error?) {
    guard active === session else { return }
    if session.edit == nil { finish(session, error: error); return }
    session.viewerGone = true
    if let error, session.failure == nil { session.failure = error }
    completeEditing(session)
  }

  private func completeEditing(_ session: Session) {
    guard active === session, session.edit != nil, session.viewerGone, session.worker == nil else { return }
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
      if session.pending.isEmpty { retry = nil }
      else {
        retry = { @MainActor [weak self, weak session] in
          guard let self, let session, self.active === session, session.forced == nil else { return }
          session.recoveryShown = false
          session.failure = nil
          self.startWorker(session)
        }
      }
      let changesSaved = !session.pending.isEmpty && session.pending.allSatisfy(\.accepted) && session.captureFailure == nil
      session.presentation.recover(error: changesSaved ? Failure.cleanup : failure, changesSaved: changesSaved, retry: retry) { [weak self, weak session] in
        guard let self, let session, self.active === session, session.forced == nil else { return }
        for edit in session.pending {
          do { try edit.file.remove() } catch { self.retained.append(edit.file); session.cleanupFailure = error }
        }
        session.pending.removeAll()
        session.failure = nil
        self.finish(session, error: nil)
      }
    } else if session.pending.isEmpty { finish(session, error: nil) }
  }

  /// Cleanup waits for UIKit's real hierarchy dismissal and verified deletion.
  func clear(before generation: UInt64) async throws {
    invalidate(before: generation)
    for transfer in scope.drain(before: generation) { retained.append(transfer.file) }
    if let active, active.generation < generation {
      try await withCheckedThrowingContinuation { continuation in
        active.waiters.append(continuation)
        stop(active, error: Failure.changed)
      }
    }
    let previous = retained
    retained = []
    var failure: Error?
    for file in previous {
      do { try file.remove() }
      catch { retained.append(file); failure = error }
    }
    if let failure { throw failure }
  }

  private func stop(_ session: Session, error: Error) {
    guard active === session, !session.stopping else { return }
    session.stopping = true
    session.forced = error
    session.presentation.dismiss { [weak self, weak session] in
      guard let self, let session else { return }
      if session.edit == nil { self.finish(session, error: error) }
      else { session.viewerGone = true; self.completeEditing(session) }
    }
  }

  private func finish(_ session: Session, error: Error?) {
    guard active === session else { return }
    for file in session.failedCaptureCopies {
      do { try file.remove() }
      catch { retained.append(file); session.cleanupFailure = error }
    }
    session.failedCaptureCopies.removeAll()
    var result: Result<Void, Error> = error.map { .failure($0) } ?? .success(())
    var cleanup: Result<Void, Error> = session.cleanupFailure.map { .failure($0) } ?? .success(())
    if let error = session.cleanupFailure { result = .failure(error) }
    do { try session.file.remove() }
    catch { retained.append(session.file); result = .failure(error); cleanup = .failure(error) }
    active = nil
    session.editing?.captured = nil
    session.continuation.resume(with: result)
    for waiter in session.waiters { waiter.resume(with: cleanup) }
    session.waiters.removeAll()
    for waiter in session.lifecycle { waiter.resume(with: cleanup) }
    session.lifecycle.removeAll()
  }
}
