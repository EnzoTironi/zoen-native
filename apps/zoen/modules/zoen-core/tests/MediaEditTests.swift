import Foundation
import ImageIO
import UniformTypeIdentifiers
import AVFoundation
import CoreVideo

@MainActor private final class Signal {
  var fired = false
  private var waiters: [CheckedContinuation<Void, Never>] = []
  func emit() { fired = true; waiters.forEach { $0.resume() }; waiters.removeAll() }
  func wait() async { if !fired { await withCheckedContinuation { waiters.append($0) } } }
}
@MainActor private final class Presentation: ZoenFilePreviewPresentation {
  let shown = Signal(), dismissRequested = Signal(), recovered = Signal()
  var ended: (@MainActor (Error?) -> Void)?
  var dismissals: [@MainActor () -> Void] = []
  var discard: (@MainActor () -> Void)?
  var captureFailures = 0
  func show(file: URL, title: String, ended: @escaping @MainActor (Error?) -> Void) throws { self.ended = ended; shown.emit() }
  func dismiss(completion: @escaping @MainActor () -> Void) { dismissals.append(completion); dismissRequested.emit() }
  func finishDismissal() { dismissals.removeFirst()() }
  func recover(error: Error, changesSaved: Bool, retry: (@MainActor () -> Void)?, discard: @escaping @MainActor () -> Void) { self.discard = discard; recovered.emit() }
  func captureFailed(error: Error) { captureFailures += 1 }
}
@MainActor private final class Completion {
  var editing: ZoenFilePreview.Editing?
  var result: Result<Void, Error>?
  func run(_ work: () async throws -> Void) async { do { try await work(); result = .success(()) } catch { result = .failure(error) } }
}
private actor Saves {
  let gate: ZoenMediaEditGate
  var calls = 0
  var accepted: [Data] = []
  var files: [URL] = []
  var held: Bool
  private var releaseWaiters: [CheckedContinuation<Void, Never>] = []
  private var callWaiters: [CheckedContinuation<Void, Never>] = []
  init(gate: ZoenMediaEditGate, held: Bool = false) { self.gate = gate; self.held = held }
  func save(_ file: ZoenPreviewFile) async throws {
    calls += 1; files.append(file.url); callWaiters.forEach { $0.resume() }; callWaiters.removeAll()
    if held { await withCheckedContinuation { releaseWaiters.append($0) } }
    let data = try gate.admitted { try Data(contentsOf: file.url) }
    accepted.append(data)
  }
  func waitForCall() async { if calls == 0 { await withCheckedContinuation { callWaiters.append($0) } } }
  func release() { held = false; releaseWaiters.forEach { $0.resume() }; releaseWaiters.removeAll() }
}
@MainActor private final class Fixture {
  let root: URL
  init() throws {
    root = FileManager.default.temporaryDirectory.appendingPathComponent("zoen-media-edit-tests-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
  }
  func file(_ name: String) throws -> ZoenPreviewFile {
    let directory = root.appendingPathComponent(name)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    let file = ZoenPreviewFile(url: directory.appendingPathComponent("photo.png"), directory: directory)
    try png(file.url)
    return file
  }
  func png(_ url: URL) throws {
    let pixels = Data([240, 40, 80, 255, 20, 180, 230, 255])
    guard let provider = CGDataProvider(data: pixels as CFData),
          let image = CGImage(width: 2, height: 1, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: 8,
            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
            provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent),
          let output = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) else { throw CocoaError(.fileWriteUnknown) }
    CGImageDestinationAddImage(output, image, nil)
    guard CGImageDestinationFinalize(output) else { throw CocoaError(.fileWriteUnknown) }
  }
  func movie(_ url: URL) async throws {
    let writer = try AVAssetWriter(outputURL: url, fileType: .mov)
    let input = AVAssetWriterInput(mediaType: .video, outputSettings: [AVVideoCodecKey: AVVideoCodecType.h264, AVVideoWidthKey: 32, AVVideoHeightKey: 32])
    let adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input, sourcePixelBufferAttributes: [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32ARGB, kCVPixelBufferWidthKey as String: 32, kCVPixelBufferHeightKey as String: 32])
    writer.add(input)
    guard writer.startWriting() else { throw writer.error ?? CocoaError(.fileWriteUnknown) }
    writer.startSession(atSourceTime: .zero)
    var buffer: CVPixelBuffer?
    guard CVPixelBufferCreate(kCFAllocatorDefault, 32, 32, kCVPixelFormatType_32ARGB, nil, &buffer) == kCVReturnSuccess, let buffer else { throw CocoaError(.fileWriteUnknown) }
    CVPixelBufferLockBaseAddress(buffer, [])
    memset(CVPixelBufferGetBaseAddress(buffer), 80, CVPixelBufferGetDataSize(buffer))
    CVPixelBufferUnlockBaseAddress(buffer, [])
    for frame in 0..<2 {
      while !input.isReadyForMoreMediaData { await Task.yield() }
      guard adaptor.append(buffer, withPresentationTime: CMTime(value: Int64(frame), timescale: 1)) else { throw writer.error ?? CocoaError(.fileWriteUnknown) }
    }
    input.markAsFinished(); writer.endSession(atSourceTime: CMTime(value: 2, timescale: 1))
    await writer.finishWriting()
    guard writer.status == .completed else { throw writer.error ?? CocoaError(.fileWriteUnknown) }
  }
  func remove() { try? FileManager.default.removeItem(at: root) }
}
@main @MainActor struct MediaEditTests {
  private static func begin(owner: ZoenFilePreview, file: ZoenPreviewFile, store: Saves, presentation: Presentation, result: Completion) -> Task<Void, Never> {
    let ticket = owner.register(file: file, generation: 1)
    let context = ZoenFilePreview.MediaEditContext(kind: .image, maximumBytes: 4096) { file in try await store.save(file) }
    return Task { @MainActor in
      await result.run {
        try await owner.open(id: ticket, title: "Synthetic photo", edit: nil, mediaEdit: context) { editing in result.editing = editing; return presentation }
      }
    }
  }
  static func main() async throws {
    try await copiedBytesAndNativeCancel()
    try await cancellationDrainsWorkerWithoutSavingQueuedCopy()
    try await ordinaryCloseCancelsDraftEditor()
    try await badCaptureOffersExplicitDiscard()
    try await captureFormatsAndCleanup()
    try gateAndNativeBackgroundAdmission()
    print("Synthetic media edit tests passed: 6 scenarios (exact copied bytes, draft cancellation/drain, ordinary close, malformed capture recovery, real PNG/MOV formats and cleanup, background gates).")
  }
  static func copiedBytesAndNativeCancel() async throws {
    let f = try Fixture(); defer { f.remove() }
    let original = try f.file("original"), callback = try f.file("framework-copy")
    let expected = try Data(contentsOf: callback.url)
    let owner = ZoenFilePreview(), gate = ZoenMediaEditGate(), store = Saves(gate: gate)
    let p = Presentation(), result = Completion()
    let task = begin(owner: owner, file: original, store: store, presentation: p, result: result)
    await p.shown.wait(); result.editing!.receive(callback.url); await store.waitForCall()
    p.ended?(nil); await task.value
    guard case .success? = result.result else { preconditionFailure("Saved photo did not close cleanly") }
    let accepted = await store.accepted, copies = await store.files
    precondition(accepted == [expected] && copies.count == 1 && copies[0] != callback.url)
    precondition(FileManager.default.fileExists(atPath: callback.url.path), "Framework/original input was changed")
    precondition(!FileManager.default.fileExists(atPath: copies[0].path) && !FileManager.default.fileExists(atPath: original.url.path), "Owned viewer/edit lease survived dismissal")
    // No edited-copy callback is generated when the native user cancels markup.
    let p2 = Presentation(), r2 = Completion(), s2 = Saves(gate: ZoenMediaEditGate())
    let t2 = begin(owner: owner, file: try f.file("cancel-viewer"), store: s2, presentation: p2, result: r2)
    await p2.shown.wait(); p2.ended?(nil); await t2.value
    let calls = await s2.calls; precondition(calls == 0)
  }
  static func cancellationDrainsWorkerWithoutSavingQueuedCopy() async throws {
    let f = try Fixture(); defer { f.remove() }
    let owner = ZoenFilePreview(), gate = ZoenMediaEditGate(), store = Saves(gate: gate, held: true)
    let p = Presentation(), result = Completion()
    let task = begin(owner: owner, file: try f.file("viewer"), store: store, presentation: p, result: result)
    await p.shown.wait(); result.editing!.receive(try f.file("one").url); await store.waitForCall()
    result.editing!.receive(try f.file("two").url)
    gate.cancel(); task.cancel(); await p.dismissRequested.wait(); p.finishDismissal()
    precondition(result.result == nil, "Cancellation lost an admitted native worker")
    await store.release(); await task.value
    let calls = await store.calls, accepted = await store.accepted, copies = await store.files
    precondition(calls == 1 && accepted.isEmpty, "Cancel admitted a queued or delayed canonical save")
    precondition(copies.allSatisfy { !FileManager.default.fileExists(atPath: $0.path) })
    guard case .failure? = result.result else { preconditionFailure("Cancelled editor did not fail") }
  }
  static func ordinaryCloseCancelsDraftEditor() async throws {
    let f = try Fixture(); defer { f.remove() }
    let owner = ZoenFilePreview(), store = Saves(gate: ZoenMediaEditGate()), p = Presentation(), result = Completion()
    let task = begin(owner: owner, file: try f.file("viewer"), store: store, presentation: p, result: result)
    await p.shown.wait()
    let close = Task { try await owner.prepareForClose(before: 2) }
    await p.dismissRequested.wait(); p.finishDismissal(); try await close.value; await task.value
    let calls = await store.calls; precondition(calls == 0)
    guard case .failure? = result.result else { preconditionFailure("Ordinary close retained draft editor") }
  }
  static func badCaptureOffersExplicitDiscard() async throws {
    let f = try Fixture(); defer { f.remove() }
    let owner = ZoenFilePreview(), store = Saves(gate: ZoenMediaEditGate()), p = Presentation(), result = Completion()
    let task = begin(owner: owner, file: try f.file("viewer"), store: store, presentation: p, result: result)
    await p.shown.wait()
    let bad = f.root.appendingPathComponent("bad.png"); try Data("not an image".utf8).write(to: bad)
    result.editing!.receive(bad); precondition(p.captureFailures == 1)
    p.ended?(nil); await p.recovered.wait(); p.discard?(); await task.value
    guard case .success? = result.result else { preconditionFailure("Explicit media discard failed to drain") }
    let calls = await store.calls; precondition(calls == 0 && FileManager.default.fileExists(atPath: bad.path))
  }
  static func captureFormatsAndCleanup() async throws {
    let f = try Fixture(); defer { f.remove() }
    let source = try f.file("source")
    guard case .success(let image) = ZoenMediaEditCapture.stage(source.url, kind: .image, maximumBytes: 4096) else { preconditionFailure("Real PNG was rejected") }
    let imageMetadata = try await ZoenMediaEditCapture.validate(image, kind: .image, maximumBytes: 4096)
    precondition(imageMetadata.mime == "image/png" && imageMetadata.fileExtension == "png")
    let copiedImage = try Data(contentsOf: image.url), sourceImage = try Data(contentsOf: source.url)
    precondition(copiedImage == sourceImage)
    try image.remove()
    let link = f.root.appendingPathComponent("link.png"); try FileManager.default.createSymbolicLink(at: link, withDestinationURL: source.url)
    for path in [link, f.root] {
      guard case .failure(let failure) = ZoenMediaEditCapture.stage(path, kind: .image, maximumBytes: 4096) else { preconditionFailure("Unsafe callback admitted") }
      precondition(failure.retained == nil)
    }
    guard case .failure = ZoenMediaEditCapture.stage(source.url, kind: .image, maximumBytes: 1) else { preconditionFailure("Size limit ignored") }
    let movie = f.root.appendingPathComponent("movie.mov"); try await f.movie(movie)
    guard case .success(let video) = ZoenMediaEditCapture.stage(movie, kind: .video, maximumBytes: 1 << 20) else { preconditionFailure("Real MOV rejected") }
    let videoMetadata = try await ZoenMediaEditCapture.validate(video, kind: .video, maximumBytes: 1 << 20)
    precondition(videoMetadata.mime == "video/quicktime")
    let copiedVideo = try Data(contentsOf: video.url), sourceVideo = try Data(contentsOf: movie)
    precondition(copiedVideo == sourceVideo); try video.remove()
    let malformed = f.root.appendingPathComponent("invalid.mov"); try Data("not a movie".utf8).write(to: malformed)
    guard case .success(let staged) = ZoenMediaEditCapture.stage(malformed, kind: .video, maximumBytes: 4096) else { preconditionFailure("Expected bounded copy before async validation") }
    defer { try? staged.remove() }
    do { _ = try await ZoenMediaEditCapture.validate(staged, kind: .video, maximumBytes: 4096); preconditionFailure("Invalid video admitted for save") } catch {}
    precondition(ZoenMediaEditCapture.kind(mime: "image/gif") == nil)
    precondition(ZoenMediaEditCapture.kind(mime: "application/pdf") == nil)
  }
  static func gateAndNativeBackgroundAdmission() throws {
    let lifetime = ZoenMediaEditLifetime(), gate = ZoenMediaEditGate(), id = UUID()
    try lifetime.register(id, gate: gate)
    var commits = 0; try gate.admitted { commits += 1 }
    let retired = lifetime.retire()
    do { try gate.admitted { commits += 1 }; preconditionFailure("Background allowed late CAS") } catch is CancellationError {}
    do { try lifetime.register(UUID(), gate: ZoenMediaEditGate()); preconditionFailure("Background allowed new editor") } catch is CancellationError {}
    lifetime.activate(); let fresh = ZoenMediaEditGate(), token = UUID(); try lifetime.register(token, gate: fresh)
    precondition(retired == [id] && !retired.contains(token), "Delayed background cleanup captured a newer foreground token")
    try fresh.admitted { commits += 1 }
    lifetime.cancel(token)
    do { try fresh.admitted { commits += 1 }; preconditionFailure("Cancelled token admitted") } catch is CancellationError {}
    precondition(commits == 2); lifetime.remove(id); lifetime.remove(token)
  }
}
