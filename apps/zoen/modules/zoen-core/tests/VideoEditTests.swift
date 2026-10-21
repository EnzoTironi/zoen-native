import AVFoundation
import Foundation

@MainActor private final class VideoSignal {
  var emitted = false
  private var waiters: [CheckedContinuation<Void, Never>] = []
  func signal() { emitted = true; let waiting = waiters; waiters = []; waiting.forEach { $0.resume() } }
  func wait() async { if !emitted { await withCheckedContinuation { waiters.append($0) } } }
}
@MainActor private final class VideoPresentation: ZoenFilePreviewPresentation {
  let shown = VideoSignal(), dismissRequested = VideoSignal(), recoveryShown = VideoSignal()
  var ended: (@MainActor (Error?) -> Void)?
  var retry: (@MainActor () -> Void)?
  var discard: (@MainActor () -> Void)?
  var dismissals: [@MainActor () -> Void] = []
  func show(file: URL, title: String, ended: @escaping @MainActor (Error?) -> Void) throws { self.ended = ended; shown.signal() }
  func dismiss(completion: @escaping @MainActor () -> Void) { dismissals.append(completion); dismissRequested.signal() }
  func completeDismissal() { dismissals.removeFirst()() }
  func recover(error: Error, changesSaved: Bool, retry: (@MainActor () -> Void)?, discard: @escaping @MainActor () -> Void) {
    self.retry = retry; self.discard = discard; recoveryShown.signal()
  }
}
@MainActor private final class VideoCompletion {
  var result: Result<Void, Error>?
  var editing: ZoenFilePreview.Editing?
  func run(_ work: () async throws -> Void) async { do { try await work(); result = .success(()) } catch { result = .failure(error) } }
}
private actor VideoProbe {
  var destination: URL?
  var checks = 0
  var revoked = false
  var accepted: [Data] = []
  var attemptedFiles: [URL] = []
  var rejectFirstSave = false
  private var opened = false, released = false
  private var enteredWaiters: [CheckedContinuation<Void, Never>] = []
  private var releaseWaiters: [CheckedContinuation<Void, Never>] = []
  init(rejectFirstSave: Bool = false) { self.rejectFirstSave = rejectFirstSave }
  func authorize() throws { checks += 1; if revoked { throw ZoenFilePreview.Failure.changed } }
  func revoke() { revoked = true }
  func mark(_ url: URL) { destination = url; opened = true; let waiting = enteredWaiters; enteredWaiters = []; waiting.forEach { $0.resume() } }
  func hold(_ url: URL) async { mark(url); if !released { await withCheckedContinuation { releaseWaiters.append($0) } } }
  func waitUntilEntered() async { if !opened { await withCheckedContinuation { enteredWaiters.append($0) } } }
  func release() { released = true; let waiting = releaseWaiters; releaseWaiters = []; waiting.forEach { $0.resume() } }
  func save(_ file: ZoenPreviewFile) throws {
    attemptedFiles.append(file.url)
    if rejectFirstSave && attemptedFiles.count == 1 { throw ZoenFilePreview.Failure.changed }
    accepted.append(try Data(contentsOf: file.url))
  }
}

/// Runs the actual FilePreview/capture owner and real renderer on macOS. A
/// synthetic presentation stands in for UIKit only; no Rust CAS/UI claim.
@main @MainActor struct VideoEditTests {
  private static func file(_ bytes: Data) throws -> ZoenPreviewFile {
    let dir = URL.temporaryDirectory.appendingPathComponent("VideoEditTests-" + UUID().uuidString)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    let result = ZoenPreviewFile(url: dir.appendingPathComponent("source.mp4"), directory: dir)
    try bytes.write(to: result.url); return result
  }
  private static func begin(_ owner: ZoenFilePreview, source: ZoenPreviewFile, probe: VideoProbe,
                            presentation: VideoPresentation, completion: VideoCompletion) -> Task<Void, Never> {
    let id = owner.register(file: source, generation: 1)
    let context = ZoenFilePreview.MediaEditContext(kind: .video, maximumBytes: 4 << 20,
      authorize: { try await probe.authorize() }, save: { try await probe.save($0) })
    return Task { @MainActor in await completion.run {
      try await owner.open(id: id, title: "Synthetic video", edit: nil, mediaEdit: context) { editing in
        completion.editing = editing; return presentation
      }
    } }
  }
  static func main() async throws {
    let source = URL(fileURLWithPath: CommandLine.arguments[1])
    let bytes = try Data(contentsOf: source)
    try await canonicalTrimAndMute(bytes)
    try await revokedAfterRenderCannotSave(bytes)
    try await cancellationDrainsPartialExportBeforeDeletingInput(bytes)
    try await failedCleanupRemainsOwnedAndBlocksNextPreview(bytes)
    try await rejectedSaveRetriesSameFileWithoutRenderingAgain(bytes)
    try await overlappingAndRetiredRendererCannotCapture(bytes)
    try await malformedAndOversizeRenderCannotSave(bytes)
    let unchanged = try Data(contentsOf: source)
    precondition(unchanged == bytes, "Source fixture changed")
    print("Video edit owner tests passed: 7 scenarios; actual trim/audio removal + canonical same-file save/reopen, revoked checkpoints, partial export cancellation/drain, denied cleanup/retry, rejected save recovery, overlapping/retired callbacks, malformed/oversize rejection. Host AVFoundation, synthetic presentation/save callback; no iOS UI/Rust CAS claim.")
  }
  static func canonicalTrimAndMute(_ bytes: Data) async throws {
    for muted in [false, true] {
      let owner = ZoenFilePreview(), p = VideoPresentation(), c = VideoCompletion(), probe = VideoProbe()
      let source = try file(bytes), asset = AVURLAsset(url: source.url)
      let opening = begin(owner, source: source, probe: probe, presentation: p, completion: c)
      await p.shown.wait()
      try await c.editing!.receiveVideo { destination, maximum in
        await probe.mark(destination)
        precondition(destination.deletingLastPathComponent().deletingLastPathComponent().lastPathComponent == "ZoenMedia")
        precondition(!FileManager.default.fileExists(atPath: destination.path))
        _ = try await ZoenTelegramVideoRenderer.render(asset: asset,
          range: CMTimeRange(start: CMTime(seconds: 0.7, preferredTimescale: 600), duration: CMTime(seconds: 1.2, preferredTimescale: 600)),
          sendAsGif: muted, destination: destination, maximumBytes: maximum)
      }
      p.ended?(nil); await opening.value
      guard case .success? = c.result else { preconditionFailure("Explicit video save did not settle") }
      let accepted = await probe.accepted, attempts = await probe.attemptedFiles
      let destination = await probe.destination, checks = await probe.checks
      precondition(accepted.count == 1 && attempts == [destination!], "Renderer introduced a second plaintext copy")
      precondition(checks == 3, "Missing authorization checkpoint")
      precondition(!FileManager.default.fileExists(atPath: destination!.deletingLastPathComponent().path))
      precondition(!FileManager.default.fileExists(atPath: source.directory.path))
      let reopened = try file(accepted[0]); defer { try? reopened.remove() }
      let recovered = AVURLAsset(url: reopened.url), duration = try await recovered.load(.duration)
      let audio = try await recovered.loadTracks(withMediaType: .audio)
      precondition(abs(duration.seconds - 1.2) < 0.04)
      precondition(muted ? audio.isEmpty : !audio.isEmpty, "Mute must remove actual audio tracks")
    }
  }
  static func revokedAfterRenderCannotSave(_ bytes: Data) async throws {
    let owner = ZoenFilePreview(), p = VideoPresentation(), c = VideoCompletion(), probe = VideoProbe()
    let source = try file(bytes), opening = begin(owner, source: source, probe: probe, presentation: p, completion: c)
    await p.shown.wait()
    do {
      try await c.editing!.receiveVideo { url, _ in await probe.mark(url); try bytes.write(to: url); await probe.revoke() }
      preconditionFailure("Revoked render was captured")
    } catch {}
    let destination = await probe.destination
    precondition(!FileManager.default.fileExists(atPath: destination!.deletingLastPathComponent().path))
    p.ended?(nil); await opening.value
    let accepted = await probe.accepted; precondition(accepted.isEmpty)
  }
  static func cancellationDrainsPartialExportBeforeDeletingInput(_ bytes: Data) async throws {
    let owner = ZoenFilePreview(), p = VideoPresentation(), c = VideoCompletion(), probe = VideoProbe()
    let source = try file(bytes), opening = begin(owner, source: source, probe: probe, presentation: p, completion: c)
    await p.shown.wait()
    let rendering = Task { @MainActor in
      do { try await c.editing!.receiveVideo { url, _ in
        try Data("partial".utf8).write(to: url); await probe.hold(url); try Task.checkCancellation()
      }; preconditionFailure("Cancelled render succeeded") } catch {}
    }
    await probe.waitUntilEntered()
    opening.cancel(); await p.dismissRequested.wait(); p.completeDismissal()
    precondition(c.result == nil, "Dismissal forgot active export")
    precondition(FileManager.default.fileExists(atPath: source.url.path), "Export input was removed before drain")
    await probe.release(); await rendering.value; await opening.value
    let destination = await probe.destination, accepted = await probe.accepted
    precondition(accepted.isEmpty && !FileManager.default.fileExists(atPath: destination!.deletingLastPathComponent().path))
    precondition(!FileManager.default.fileExists(atPath: source.directory.path))
    guard case .failure? = c.result else { preconditionFailure("Cancelled owner should fail") }
  }
  static func failedCleanupRemainsOwnedAndBlocksNextPreview(_ bytes: Data) async throws {
    let owner = ZoenFilePreview(), p = VideoPresentation(), c = VideoCompletion(), probe = VideoProbe()
    let source = try file(bytes), opening = begin(owner, source: source, probe: probe, presentation: p, completion: c)
    await p.shown.wait()
    do {
      try await c.editing!.receiveVideo { url, _ in
        await probe.mark(url); try Data("partial".utf8).write(to: url)
        try FileManager.default.setAttributes([.posixPermissions: 0o000], ofItemAtPath: url.deletingLastPathComponent().path)
        throw ZoenFilePreview.Failure.editedMedia
      }
      preconditionFailure("Failed render succeeded")
    } catch {}
    let destination = await probe.destination, directory = destination!.deletingLastPathComponent()
    defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path); try? FileManager.default.removeItem(at: directory) }
    p.ended?(nil); await opening.value
    guard case .failure? = c.result else { preconditionFailure("Denied cleanup was ignored") }
    do { try await owner.clear(before: 2); preconditionFailure("Denied lease was lost") } catch {}
    let p2 = VideoPresentation(), c2 = VideoCompletion(), source2 = try file(bytes)
    let rejected = begin(owner, source: source2, probe: VideoProbe(), presentation: p2, completion: c2)
    await rejected.value
    guard case .failure? = c2.result else { preconditionFailure("Retained output failed to block another owner") }
    precondition(!p2.shown.emitted)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path)
    try await owner.clear(before: 2)
    precondition(!FileManager.default.fileExists(atPath: directory.path))
  }
  static func rejectedSaveRetriesSameFileWithoutRenderingAgain(_ bytes: Data) async throws {
    let owner = ZoenFilePreview(), p = VideoPresentation(), c = VideoCompletion(), probe = VideoProbe(rejectFirstSave: true)
    let source = try file(bytes), opening = begin(owner, source: source, probe: probe, presentation: p, completion: c)
    await p.shown.wait()
    try await c.editing!.receiveVideo { url, _ in await probe.mark(url); try bytes.write(to: url) }
    p.ended?(nil); await p.recoveryShown.wait()
    let destination = await probe.destination
    precondition(FileManager.default.fileExists(atPath: destination!.path), "Rejected save discarded its exact retry file")
    precondition(p.retry != nil); p.retry?(); await opening.value
    guard case .success? = c.result else { preconditionFailure("Exact save retry did not recover") }
    let attempts = await probe.attemptedFiles, accepted = await probe.accepted
    precondition(attempts == [destination!, destination!] && accepted == [bytes])
    precondition(!FileManager.default.fileExists(atPath: destination!.path))
  }
  static func overlappingAndRetiredRendererCannotCapture(_ bytes: Data) async throws {
    let owner = ZoenFilePreview(), p = VideoPresentation(), c = VideoCompletion(), probe = VideoProbe()
    let source = try file(bytes), opening = begin(owner, source: source, probe: probe, presentation: p, completion: c)
    await p.shown.wait()
    let retained = c.editing!
    let rendering = Task { @MainActor in try await retained.receiveVideo { url, _ in
      await probe.hold(url); try Task.checkCancellation(); try bytes.write(to: url)
    } }
    await probe.waitUntilEntered()
    do { try await retained.receiveVideo { _, _ in preconditionFailure("Second renderer entered") }; preconditionFailure("Concurrent render accepted") } catch {}
    await probe.release(); try await rendering.value
    p.ended?(nil); await opening.value
    do { try await retained.receiveVideo { _, _ in preconditionFailure("Retired renderer entered") }; preconditionFailure("Retired callback accepted") } catch {}
  }
  static func malformedAndOversizeRenderCannotSave(_ bytes: Data) async throws {
    for data in [Data("not a movie".utf8), Data(repeating: 1, count: (4 << 20) + 1)] {
      let owner = ZoenFilePreview(), p = VideoPresentation(), c = VideoCompletion(), probe = VideoProbe()
      let source = try file(bytes), opening = begin(owner, source: source, probe: probe, presentation: p, completion: c)
      await p.shown.wait()
      do { try await c.editing!.receiveVideo { url, _ in await probe.mark(url); try data.write(to: url) }; preconditionFailure("Invalid output accepted") } catch {}
      let destination = await probe.destination
      precondition(!FileManager.default.fileExists(atPath: destination!.deletingLastPathComponent().path))
      p.ended?(nil); await opening.value
      let accepted = await probe.accepted; precondition(accepted.isEmpty)
    }
  }
}
