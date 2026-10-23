import Foundation

@MainActor extension ZoenFilePreview {
  final class ReadOnlyRegistration: @unchecked Sendable {
    // Accessed only while the containing Scope lock is held.
    var retired = false
  }
  /// Disposable pages belong to this same preview session, including the first
  /// page. A virtualized source row never becomes the owner of these copies.
  @MainActor final class ReadOnlyCopies {
    nonisolated let registration = ReadOnlyRegistration()
    private var files: [String: ZoenPreviewFile] = [:]
    var admit: (() throws -> Void)?
    var failed: ((ZoenPreviewFile) -> Void)?
    var transfer: ((UUID) throws -> ZoenPreviewFile)?
    var pending: (() -> [ZoenPreviewFile])?
    var live = false
    var bound = false

    /// The actor registers BEFORE it returns. Forced clear can therefore own a
    /// copy even while this MainActor continuation is still queued.
    func accept(_ ticket: UUID, key: String) throws -> URL {
      guard let transfer else { throw Failure.changed }
      let file = try transfer(ticket)
      try retain(file, key: key)
      return file.url
    }

    func retain(_ file: ZoenPreviewFile, key: String) throws {
      do {
        guard live, files[key] == nil, files.count < 3 else { throw Failure.changed }
        try admit?()
        files[key] = file
      } catch {
        do { try file.remove() } catch { failed?(file) }
        throw error
      }
    }

    /// Call only after the corresponding display requests/player are retired.
    func release(except keys: Set<String>) throws {
      var failure: Error?
      for key in Array(files.keys) where !keys.contains(key) {
        guard let file = files.removeValue(forKey: key) else { continue }
        do { try file.remove() } catch { failed?(file); failure = error }
      }
      if let failure { throw failure }
    }

    func finish() throws {
      live = false
      // A cancelled async page may return after dismissal. Its failed deletion
      // must still reach this owner's retained-cleanup list.
      defer { admit = nil }
      var failure: Error?
      do { try release(except: []) } catch { failure = error }
      for file in pending?() ?? [] {
        do { try file.remove() } catch { failed?(file); failure = error }
      }
      pending = nil
      if let failure { throw failure }
    }
  }

  enum Failure: Error, LocalizedError {
    case changed, busy, unavailable, cleanup, editedPDF, editedMedia, recovered
    var errorDescription: String? {
      switch self {
      case .changed: NSLocalizedString("This preview is no longer available.", comment: "Native preview session changed")
      case .busy: NSLocalizedString("Close the current preview and try again.", comment: "Native preview is already open")
      case .unavailable: NSLocalizedString("This file could not be previewed. Try again.", comment: "Native file preview unavailable")
      case .cleanup: NSLocalizedString("The temporary preview could not be removed. Try again.", comment: "Native preview cleanup failed")
      case .editedMedia: NSLocalizedString("The edited photo or video could not be read. Try saving your changes again.", comment: "Native edited media capture failed")
      case .editedPDF: NSLocalizedString("The edited PDF could not be read. Try saving your changes again.", comment: "Native PDF edit could not be retained")
      case .recovered: NSLocalizedString("These changes are only on this device. Save them to update the shared file.", comment: "Recovered private PDF edit has not been published")
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
    let canCapture: Bool
    let retain: (@Sendable (ZoenPreviewFile) throws -> Void)?
    let discard: (@Sendable () async throws -> Void)?
    let save: @Sendable (ZoenPreviewFile, EditBase) async throws -> EditBase

    init(base: EditBase, maximumBytes: UInt64, canCapture: Bool = true,
         retain: (@Sendable (ZoenPreviewFile) throws -> Void)? = nil,
         discard: (@Sendable () async throws -> Void)? = nil,
         save: @escaping @Sendable (ZoenPreviewFile, EditBase) async throws -> EditBase) {
      self.base = base; self.maximumBytes = maximumBytes
      self.canCapture = canCapture
      self.retain = retain; self.discard = discard; self.save = save
    }
  }

  struct MediaEditContext: Sendable {
    let kind: ZoenMediaEditCapture.Kind
    let maximumBytes: UInt64
    let authorize: @Sendable () async throws -> Void
    let save: @Sendable (ZoenPreviewFile) async throws -> Void

    init(kind: ZoenMediaEditCapture.Kind, maximumBytes: UInt64,
         authorize: @escaping @Sendable () async throws -> Void = {},
         save: @escaping @Sendable (ZoenPreviewFile) async throws -> Void) {
      self.kind = kind; self.maximumBytes = maximumBytes
      self.authorize = authorize; self.save = save
    }
  }

  @MainActor final class Editing {
    private let maximumBytes: UInt64
    private let mediaKind: ZoenMediaEditCapture.Kind?
    let allowsEditing: Bool
    var captured: ((Result<ZoenPreviewFile, ZoenPDFEditCapture.Failure>) -> Void)?
    var renderVideo: ((@escaping @Sendable (URL, UInt64) async throws -> Void) async throws -> Void)?
    init(maximumBytes: UInt64, allowsEditing: Bool, mediaKind: ZoenMediaEditCapture.Kind? = nil) {
      self.maximumBytes = maximumBytes; self.allowsEditing = allowsEditing; self.mediaKind = mediaKind
    }
    func receive(_ url: URL) {
      guard allowsEditing else { return }
      if let mediaKind { captured?(ZoenMediaEditCapture.stage(url, kind: mediaKind, maximumBytes: maximumBytes)) }
      else { captured?(ZoenPDFEditCapture.stage(url, maximumBytes: maximumBytes)) }
    }
    func receiveImage(_ png: Data) {
      guard allowsEditing, mediaKind == .image else { return }
      captured?(ZoenMediaEditCapture.stageImage(png, maximumBytes: maximumBytes))
    }
    /// The renderer receives the existing owner's exact destination. It must
    /// never allocate a second plaintext file or turn UI state into a save ACK.
    func receiveVideo(_ render: @escaping @Sendable (URL, UInt64) async throws -> Void) async throws {
      guard allowsEditing, mediaKind == .video, let renderVideo else { throw Failure.changed }
      try await renderVideo(render)
    }
  }

}
