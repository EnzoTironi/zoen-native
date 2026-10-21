import Foundation

@MainActor private final class CompletionPresentation: ZoenFilePreviewPresentation {
  var ended: (@MainActor (Error?) -> Void)?
  var dismissal: (@MainActor () -> Void)?
  var unavailable = false
  func show(file: URL, title: String, ended: @escaping @MainActor (Error?) -> Void) throws {
    if unavailable { throw ZoenFilePreview.Failure.unavailable }
    self.ended = ended
  }
  func dismiss(completion: @escaping @MainActor () -> Void) { dismissal = completion }
}

@main @MainActor struct ConversationGalleryCompletionTests {
  private static func until(_ condition: () -> Bool) async {
    for _ in 0..<10000 { if condition() { return }; await Task.yield() }
    preconditionFailure("Actual presentation owner did not settle")
  }
  private static func file() throws -> ZoenPreviewFile {
    let directory = URL.temporaryDirectory.appendingPathComponent("zoen-gallery-completion-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    let url = directory.appendingPathComponent("synthetic.bin")
    try Data("Synthetic read-only presentation bytes".utf8).write(to: url)
    return .init(url: url, directory: directory)
  }
  private static func cancellation(cleanupFails: Bool) async throws {
    let owner = ZoenFilePreview(), copy = try file(), presentation = CompletionPresentation()
    defer {
      try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: copy.directory.path)
      try? copy.remove()
    }
    let copies = ZoenFilePreview.ReadOnlyCopies()
    let ticket = try owner.registerReadOnly(file: copy, generation: 1, owner: copies.registration)
    var result: Result<Void, Error>?
    let task = Task {
      do {
        try await ZoenConversationGalleryCompletion.awaitDismissal {
          try await owner.open(id: ticket, title: "QA", edit: nil, readOnlyCopies: copies) { _ in presentation }
        }
        result = .success(())
      } catch { result = .failure(error) }
    }
    await until { presentation.ended != nil }
    if cleanupFails {
      try FileManager.default.setAttributes([.posixPermissions: 0o000], ofItemAtPath: copy.directory.path)
    }
    task.cancel()
    await until { presentation.dismissal != nil }
    for _ in 0..<20 { await Task.yield() }
    precondition(result == nil, "Cancellation completed before actual hierarchy dismissal")
    _ = try FileManager.default.attributesOfItem(atPath: copy.directory.path)
    presentation.dismissal?()
    await task.value
    if cleanupFails {
      guard case .failure(let error)? = result, let error = error as? CocoaError,
            [.fileReadNoPermission, .fileWriteNoPermission].contains(error.code) else {
        preconditionFailure("Cleanup failure was swallowed as benign cancellation: \(String(describing: result))")
      }
      try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: copy.directory.path)
      precondition(FileManager.default.fileExists(atPath: copy.url.path), "Failed deletion lost its owned bytes")
      try await owner.clear(before: 2)
    } else {
      guard case .success? = result else { preconditionFailure("Expected lifecycle close, got \(String(describing: result))") }
    }
    precondition(!FileManager.default.fileExists(atPath: copy.directory.path))
  }
  private static func realFailures() async throws {
    let owner = ZoenFilePreview(), copy = try file(), presentation = CompletionPresentation()
    presentation.unavailable = true
    let ticket = owner.register(file: copy, generation: 1)
    do {
      try await ZoenConversationGalleryCompletion.awaitDismissal {
        try await owner.open(id: ticket, title: "QA") { presentation }
      }
      preconditionFailure("Real presenter failure was suppressed")
    } catch { guard case ZoenFilePreview.Failure.unavailable = error else { throw error } }
    precondition(!FileManager.default.fileExists(atPath: copy.directory.path))
    let lookalike = NSError(domain: "synthetic-presentation", code: 1,
      userInfo: [NSLocalizedDescriptionKey: "cancelled CancellationError operation"])
    do {
      try await ZoenConversationGalleryCompletion.awaitDismissal { throw lookalike }
      preconditionFailure("Localized text was mistaken for typed cancellation")
    } catch { precondition((error as NSError) === lookalike) }
  }
  static func main() async throws {
    try await cancellation(cleanupFails: false)
    print("PASS: native cancellation waits hierarchy and verified copy removal, then completes normally")
    try await cancellation(cleanupFails: true)
    print("PASS: real chmod000 cleanup error overrides cancellation and remains retryable")
    try await realFailures()
    print("PASS: presenter failure and cancellation-like localized text still reject")
    try await ZoenConversationGalleryCompletion.awaitDismissal { throw CancellationError() }
    print("PASS: typed cancellation before file admission completes without inventing a failure")
  }
}
