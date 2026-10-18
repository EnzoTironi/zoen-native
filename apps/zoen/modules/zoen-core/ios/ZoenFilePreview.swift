import Foundation

/// One exact directory transferred from ZoenMediaFiles, never an arbitrary JS URL.
struct ZoenPreviewFile: Sendable {
  let url: URL
  let directory: URL

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
}

/// The main-actor owner is independent of the message row and Expo's presentation
/// Promise. Its continuation ends only after actual dismissal and copy deletion.
@MainActor final class ZoenFilePreview {
  enum Failure: Error, LocalizedError {
    case changed, busy, unavailable, cleanup
    var errorDescription: String? {
      switch self {
      case .changed: NSLocalizedString("This preview is no longer available.", comment: "Native preview session changed")
      case .busy: NSLocalizedString("Close the current preview and try again.", comment: "Native preview is already open")
      case .unavailable: NSLocalizedString("This file could not be previewed. Try again.", comment: "Native file preview unavailable")
      case .cleanup: NSLocalizedString("The temporary preview could not be removed. Try again.", comment: "Native preview cleanup failed")
      }
    }
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
    let id: UUID
    let generation: UInt64
    let file: ZoenPreviewFile
    let presentation: any ZoenFilePreviewPresentation
    let continuation: CheckedContinuation<Void, Error>
    var stopping = false
    var waiters: [CheckedContinuation<Void, Error>] = []
    init(id: UUID, generation: UInt64, file: ZoenPreviewFile, presentation: any ZoenFilePreviewPresentation,
         continuation: CheckedContinuation<Void, Error>) {
      self.id = id; self.generation = generation; self.file = file; self.presentation = presentation
      self.continuation = continuation
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
    guard let transfer = scope.take(id) else { throw Failure.changed }
    let file = transfer.file
    let generation = transfer.generation
    let presentation: any ZoenFilePreviewPresentation
    do {
      guard !Task.isCancelled else { throw Failure.changed }
      guard active == nil, retained.isEmpty else { throw Failure.busy }
      guard !title.contains("\0"), title.utf8.count <= 1024 else { throw Failure.unavailable }
      presentation = try makePresentation()
    } catch {
      do { try file.remove() } catch { retained.append(file); throw error }
      throw error
    }
    let id = UUID()
    try await withTaskCancellationHandler {
      try await withCheckedThrowingContinuation { continuation in
        let session = Session(id: id, generation: generation, file: file, presentation: presentation, continuation: continuation)
        active = session
        // No suspension occurs between admission and show. UIKit receives only a
        // still-owned file, and any synchronous presentation failure ends it too.
        do {
          try scope.admit(generation) {
            try presentation.show(file: file.url, title: title) { [weak self, weak session] error in
              guard let self, let session, !session.stopping else { return }
              self.finish(session, error: error)
            }
          }
          if Task.isCancelled { stop(session, error: CancellationError()) }
        } catch { finish(session, error: error) }
      }
    } onCancel: {
      Task { @MainActor [weak self] in self?.cancel(id) }
    }
  }

  private func cancel(_ id: UUID) {
    guard let active, id == active.id else { return }
    stop(active, error: CancellationError())
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
    session.presentation.dismiss { [weak self, weak session] in
      guard let self, let session else { return }
      self.finish(session, error: error)
    }
  }

  private func finish(_ session: Session, error: Error?) {
    guard active === session else { return }
    var result: Result<Void, Error> = error.map { .failure($0) } ?? .success(())
    var cleanup: Result<Void, Error> = .success(())
    do { try session.file.remove() }
    catch { retained.append(session.file); result = .failure(error); cleanup = .failure(error) }
    active = nil
    session.continuation.resume(with: result)
    for waiter in session.waiters { waiter.resume(with: cleanup) }
    session.waiters.removeAll()
  }
}
