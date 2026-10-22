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
  func recover(error: Error, changesSaved: Bool, retry: (@MainActor () -> Void)?, keep: (@MainActor () -> Void)?, discard: @escaping @MainActor () -> Void)
  func captureFailed(error: Error)
}

extension ZoenFilePreviewPresentation {
  func recover(error: Error, changesSaved: Bool, retry: (@MainActor () -> Void)?, discard: @escaping @MainActor () -> Void) {}
  func recover(error: Error, changesSaved: Bool, retry: (@MainActor () -> Void)?, keep: (@MainActor () -> Void)?, discard: @escaping @MainActor () -> Void) {
    recover(error: error, changesSaved: changesSaved, retry: retry, discard: discard)
  }
  func captureFailed(error: Error) {}
}

/// The main-actor owner is independent of the message row and Expo's presentation
/// Promise. Its continuation ends only after actual dismissal and copy deletion.
@MainActor final class ZoenFilePreview {

  final class Scope: @unchecked Sendable {
    struct Transfer: Sendable {
      let generation: UInt64
      let file: ZoenPreviewFile
      let recovered: [ZoenPreviewFile]
      let owner: ReadOnlyRegistration?
      var files: [ZoenPreviewFile] { [file] + recovered }
    }
    private let lock = NSLock()
    private var floor: UInt64 = 0
    private var pending: [UUID: Transfer] = [:]
    func register(file: ZoenPreviewFile, generation: UInt64, recovered: [ZoenPreviewFile]) -> UUID {
      lock.lock(); defer { lock.unlock() }
      let id = UUID()
      pending[id] = Transfer(generation: generation, file: file, recovered: recovered, owner: nil)
      return id
    }
    func registerReadOnly(file: ZoenPreviewFile, generation: UInt64, owner: ReadOnlyRegistration) throws -> UUID {
      lock.lock(); defer { lock.unlock() }
      let id = UUID()
      let transfer = Transfer(generation: generation, file: file, recovered: [], owner: owner)
      guard !owner.retired, generation >= floor else {
        do { try file.remove() }
        catch { pending[id] = transfer; throw error }
        throw Failure.changed
      }
      pending[id] = transfer
      return id
    }
    func finishReadOnly(_ owner: ReadOnlyRegistration) -> [ZoenPreviewFile] {
      lock.lock(); defer { lock.unlock() }
      owner.retired = true
      let ids = pending.keys.filter { pending[$0]?.owner === owner }
      return ids.compactMap { pending.removeValue(forKey: $0) }.flatMap(\.files)
    }
    /// A late producer may fail deletion after finishReadOnly already drained
    /// its registration. Transfer that retained failure to the ordinary owner's
    /// cleanup/busy guard, rather than waiting for a future account generation.
    func drainRetiredReadOnly() -> [ZoenPreviewFile] {
      lock.lock(); defer { lock.unlock() }
      let ids = pending.keys.filter { pending[$0]?.owner?.retired == true }
      return ids.compactMap { pending.removeValue(forKey: $0) }.flatMap(\.files)
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

  final class Session {
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
    let mediaEdit: MediaEditContext?
    var editable: Bool { edit != nil || mediaEdit != nil }
    let editing: Editing?
    let readOnlyCopies: ReadOnlyCopies?
    var base: EditBase?
    var pending: [Edit] = []
    var worker: Task<Void, Never>?
    var failure: Error?
    var captureFailure: Error?
    var cleanupFailure: Error?
    var failedCaptureCopies: [ZoenPreviewFile] = []
    var recoveryShown = false
    var discardRequested = false
    var keepRequested = false
    var viewerGone = false
    var renderTask: Task<Void, Error>?
    var renderFile: ZoenPreviewFile?
    var forced: Error?
    init(id: UUID, generation: UInt64, file: ZoenPreviewFile, presentation: any ZoenFilePreviewPresentation,
         continuation: CheckedContinuation<Void, Error>, edit: EditContext?, mediaEdit: MediaEditContext?, editing: Editing?, recovered: [ZoenPreviewFile], readOnlyCopies: ReadOnlyCopies?) {
      self.id = id; self.generation = generation; self.file = file; self.presentation = presentation
      self.continuation = continuation
      self.edit = edit; self.mediaEdit = mediaEdit; self.editing = editing; self.base = edit?.base
      self.readOnlyCopies = readOnlyCopies
      pending = recovered.map { Edit(file: $0) }
      if !recovered.isEmpty { failure = Failure.recovered }
    }
  }

  nonisolated let scope = Scope()
  var active: Session?
  var retained: [ZoenPreviewFile] = []
  nonisolated init() {}

  /// Synchronous invalidation closes the actor-to-main-actor scheduling gap: an
  /// old queued presentation cannot begin after account cleanup has started.
  nonisolated func invalidate(before generation: UInt64) { scope.invalidate(before: generation) }

  /// Register on ZoenSession BEFORE leaving its actor. Cleanup can therefore own
  /// a transferred file even if its queued main-actor presentation never starts.
  nonisolated func register(file: ZoenPreviewFile, generation: UInt64, recovered: [ZoenPreviewFile] = []) -> UUID {
    scope.register(file: file, generation: generation, recovered: recovered)
  }
  nonisolated func registerReadOnly(file: ZoenPreviewFile, generation: UInt64, owner: ReadOnlyRegistration) throws -> UUID {
    try scope.registerReadOnly(file: file, generation: generation, owner: owner)
  }

  /// Return an unpresented transfer after partial native preparation fails.
  /// A concurrent clear may already own it; taking once prevents double cleanup.
  func abandon(id: UUID) throws {
    guard let transfer = scope.take(id) else { return }
    var failure: Error?
    for file in transfer.files {
      do { try file.remove() }
      catch { retained.append(file); failure = error }
    }
    if let failure { throw failure }
  }

  func open(id: UUID, title: String,
            makePresentation: @MainActor () throws -> any ZoenFilePreviewPresentation) async throws {
    try await open(id: id, title: title, edit: nil) { _ in try makePresentation() }
  }

  func open(id: UUID, title: String, edit: EditContext?, mediaEdit: MediaEditContext? = nil,
            readOnlyCopies: ReadOnlyCopies? = nil, initialKey: String = "initial",
            makePresentation: @MainActor (Editing?) throws -> any ZoenFilePreviewPresentation) async throws {
    guard let transfer = scope.take(id) else { throw Failure.changed }
    let file = transfer.file
    let generation = transfer.generation
    let presentation: any ZoenFilePreviewPresentation
    let editing = edit.map { Editing(maximumBytes: $0.maximumBytes, allowsEditing: $0.canCapture) }
      ?? mediaEdit.map { Editing(maximumBytes: $0.maximumBytes, allowsEditing: true, mediaKind: $0.kind) }
    do {
      guard edit == nil || mediaEdit == nil,
            readOnlyCopies == nil || (edit == nil && mediaEdit == nil && transfer.recovered.isEmpty && readOnlyCopies?.bound == false) else { throw Failure.unavailable }
      if let owner = transfer.owner { guard owner === readOnlyCopies?.registration else { throw Failure.changed } }
      guard !Task.isCancelled else { throw Failure.changed }
      retained.append(contentsOf: scope.drainRetiredReadOnly())
      guard active == nil, retained.isEmpty else { throw Failure.busy }
      guard transfer.recovered.isEmpty || edit != nil else { throw Failure.unavailable }
      guard !title.contains("\0"), title.utf8.count <= 1024 else { throw Failure.unavailable }
      presentation = try makePresentation(editing)
    } catch {
      var cleanupFailure: Error?
      for file in transfer.files {
        do { try file.remove() } catch { retained.append(file); cleanupFailure = error }
      }
      if let cleanupFailure { throw cleanupFailure }
      throw error
    }
    let id = UUID()
    try await withTaskCancellationHandler {
      try await withCheckedThrowingContinuation { continuation in
        let session = Session(id: id, generation: generation, file: file, presentation: presentation,
                              continuation: continuation, edit: edit, mediaEdit: mediaEdit, editing: editing, recovered: transfer.recovered, readOnlyCopies: readOnlyCopies)
        active = session
        if let copies = readOnlyCopies {
          copies.bound = true
          copies.live = true
          copies.admit = { [weak self, weak session] in
            guard let self, let session, self.active === session, !session.stopping else { throw Failure.changed }
            try self.scope.admit(session.generation) {}
          }
          copies.failed = { [weak self] file in self?.retained.append(file) }
          copies.transfer = { [weak self, weak copies] ticket in
            guard let self, let copies, let transfer = self.scope.take(ticket) else { throw Failure.changed }
            guard transfer.generation == generation, transfer.recovered.isEmpty, transfer.owner === copies.registration else {
              for file in transfer.files {
                do { try file.remove() } catch { self.retained.append(file) }
              }
              throw Failure.changed
            }
            return transfer.file
          }
          copies.pending = { [weak self, weak copies] in
            guard let self, let copies else { return [] }
            return self.scope.finishReadOnly(copies.registration)
          }
          do { try copies.retain(file, key: initialKey) }
          catch { finish(session, error: error); return }
        }
        editing?.captured = { [weak self, weak session] result in
          guard let self, let session else { return }
          self.capture(result, session: session)
        }
        editing?.renderVideo = { [weak self, weak session] render in
          guard let self, let session else { throw Failure.changed }
          try await self.captureVideo(render, session: session)
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

  func cancel(_ id: UUID) {
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
    guard let active else { return }
    if active.mediaEdit != nil {
      try await withCheckedThrowingContinuation { continuation in
        active.lifecycle.append(continuation)
        stop(active, error: Failure.changed)
      }
      return
    }
    guard active.edit != nil else { return }
    try await withCheckedThrowingContinuation { active.lifecycle.append($0) }
  }


  /// Cleanup waits for UIKit's real hierarchy dismissal and verified deletion.
  func clear(before generation: UInt64) async throws {
    invalidate(before: generation)
    for transfer in scope.drain(before: generation) { retained.append(contentsOf: transfer.files) }
    retained.append(contentsOf: scope.drainRetiredReadOnly())
    if let active, active.generation < generation {
      try await withCheckedThrowingContinuation { continuation in
        active.waiters.append(continuation)
        stop(active, error: Failure.changed)
      }
    }
    // Include a failed late producer that arrived during UIKit dismissal.
    retained.append(contentsOf: scope.drainRetiredReadOnly())
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
    session.renderTask?.cancel()
    session.presentation.dismiss { [weak self, weak session] in
      guard let self, let session else { return }
      if !session.editable { self.finish(session, error: error) }
      else { session.viewerGone = true; self.completeEditing(session) }
    }
  }

  func finish(_ session: Session, error: Error?) {
    guard active === session else { return }
    // Includes a recovered FIFO whose presentation failed before any worker ran.
    for edit in session.pending {
      do { try edit.file.remove() }
      catch { retained.append(edit.file); session.cleanupFailure = error }
    }
    session.pending.removeAll()
    for file in session.failedCaptureCopies {
      do { try file.remove() }
      catch { retained.append(file); session.cleanupFailure = error }
    }
    session.failedCaptureCopies.removeAll()
    var result: Result<Void, Error> = error.map { .failure($0) } ?? .success(())
    var cleanup: Result<Void, Error> = session.cleanupFailure.map { .failure($0) } ?? .success(())
    if let error = session.cleanupFailure { result = .failure(error) }
    do {
      if let copies = session.readOnlyCopies { try copies.finish() }
      else { try session.file.remove() }
    }
    catch { retained.append(session.file); result = .failure(error); cleanup = .failure(error) }
    active = nil
    session.editing?.captured = nil
    session.editing?.renderVideo = nil
    session.continuation.resume(with: result)
    for waiter in session.waiters { waiter.resume(with: cleanup) }
    session.waiters.removeAll()
    for waiter in session.lifecycle { waiter.resume(with: cleanup) }
    session.lifecycle.removeAll()
  }
}
