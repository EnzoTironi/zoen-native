import AVFoundation
import Foundation
import Speech

/// Disposable local audio leases. The original is immutable; every preview is
/// rendered from original source frames, never from an earlier lossy export.
actor ZoenVoiceEditor {
  struct Range: Codable, Sendable {
    let startFrame: Int64
    let endFrame: Int64
  }
  struct Source: Codable, Sendable {
    let id: String
    let uri: String
    let sampleRate: Double
    let frameCount: Int64
    let durationMs: Double
    let levels: [Double]
  }
  struct Render: Codable, Sendable {
    let uri: String
    let mime: String
    let frameCount: Int64
    let durationMs: Double
    let levels: [Double]
  }
  struct Word: Codable, Sendable {
    let id: String
    let text: String
    let startFrame: Int64
    let endFrame: Int64
  }
  struct Transcript: Codable, Sendable {
    let status: String
    let text: String
    let words: [Word]
    static let unavailable = Transcript(status: "unavailable", text: "", words: [])
  }
  enum Failure: Error, LocalizedError {
    case invalidAudio, invalidRanges, closed, renderFailed
    var errorDescription: String? {
      switch self {
      case .invalidAudio: NSLocalizedString("This recording could not be opened. Record it again.", comment: "Voice source validation failed")
      case .invalidRanges: NSLocalizedString("Keep at least a quarter of a second of this recording.", comment: "Invalid voice edit ranges")
      case .closed: NSLocalizedString("This voice review has closed. Open it again.", comment: "Voice lease was released")
      case .renderFailed: NSLocalizedString("The edited recording could not be prepared. Try again.", comment: "Voice AAC render failed")
      }
    }
  }
  private struct Lease: Sendable {
    let root: URL
    let source: Source?
    var revision: UInt64 = 0
    var completed: Render?
  }
  private struct Job: Sendable {
    let sourceId: String
    let cancel: @Sendable () -> Void
    let drain: @Sendable () async -> Void
  }
  private var leases: [String: Lease] = [:]
  private var closing: Set<String> = []
  private var jobs: [UUID: Job] = [:]
  private let root: URL

  init(cacheRoot: URL = URL.cachesDirectory.appending(path: "ZoenVoiceEditor", directoryHint: .isDirectory)) {
    root = cacheRoot
    // Only this component's abandoned plaintext previews, never durable media.
    try? FileManager.default.removeItem(at: cacheRoot)
  }

  private func run<T: Sendable>(sourceId: String, operation: @escaping @Sendable () async throws -> T) async throws -> T {
    let id = UUID()
    let task = Task.detached(priority: .userInitiated, operation: operation)
    jobs[id] = Job(sourceId: sourceId, cancel: { task.cancel() }, drain: { _ = try? await task.value })
    defer { jobs.removeValue(forKey: id) }
    return try await withTaskCancellationHandler { try await task.value } onCancel: { task.cancel() }
  }

  func prepare(uri: String) async throws -> Source {
    let id = UUID().uuidString
    let root = self.root.appending(path: id, directoryHint: .isDirectory)
    leases[id] = Lease(root: root, source: nil)
    do {
      let source = try await run(sourceId: id) {
        let input = try Self.selection(uri)
        try Task.checkCancellation()
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true,
          attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
        let original = root.appendingPathComponent("original.\(input.pathExtension)")
        try FileManager.default.copyItem(at: input, to: original)
        try FileManager.default.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: original.path)
        let file = try AVAudioFile(forReading: original, commonFormat: .pcmFormatFloat32, interleaved: false)
        let rate = file.processingFormat.sampleRate
        guard rate.isFinite, rate.rounded() == rate, (8_000...192_000).contains(rate), (1...2).contains(file.processingFormat.channelCount),
              file.length > 0, Double(file.length) / rate >= 0.25, Double(file.length) / rate <= 3_600 else { throw Failure.invalidAudio }
        let levels = try Self.waveform(file)
        try Task.checkCancellation()
        return Source(id: id, uri: original.absoluteString, sampleRate: rate, frameCount: file.length,
          durationMs: Double(file.length) / rate * 1_000, levels: levels)
      }
      guard leases[id] != nil, !closing.contains(id) else { throw Failure.closed }
      leases[id] = Lease(root: root, source: source)
      return source
    } catch {
      try? await release(id)
      throw error
    }
  }

  func render(sourceId: String, ranges: [Range]) async throws -> Render {
    guard !closing.contains(sourceId), var lease = leases[sourceId], let source = lease.source,
          lease.revision < UInt64.max else { throw Failure.closed }
    // Admission of any newer edit invalidates the earlier export, including
    // when validation/export of the newer edit subsequently fails.
    lease.revision += 1
    lease.completed = nil
    leases[sourceId] = lease
    let revision = lease.revision
    let count = try Self.validate(ranges: ranges, source: source)
    let output = lease.root.appendingPathComponent("\(UUID().uuidString).m4a")
    do {
      let rendered = try await run(sourceId: sourceId) {
        try Self.render(source: source, ranges: ranges, output: output)
        try Task.checkCancellation()
        let file = try AVAudioFile(forReading: output, commonFormat: .pcmFormatFloat32, interleaved: false)
        // AAC priming/padding must not become editable source time. The packet
        // table is expected to preserve the exact number of input PCM frames.
        guard file.length == count, file.processingFormat.sampleRate == source.sampleRate else { throw Failure.renderFailed }
        let size = try output.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
        guard size > 0, UInt64(size) <= ZoenMediaFiles.maximumBytes else { throw Failure.renderFailed }
        return Render(uri: output.absoluteString, mime: "audio/mp4", frameCount: count,
          durationMs: Double(count) / source.sampleRate * 1_000, levels: try Self.waveform(file))
      }
      guard var current = leases[sourceId], current.revision == revision, !closing.contains(sourceId) else { throw Failure.closed }
      current.completed = rendered
      leases[sourceId] = current
      return rendered
    } catch {
      try? FileManager.default.removeItem(at: output)
      throw error
    }
  }

  /// Explicit user action only. Apple may download the requested language model;
  /// audio itself never leaves this process and there is no server fallback.
  func transcribe(sourceId: String, locale: String) async throws -> Transcript {
    guard !closing.contains(sourceId), let source = leases[sourceId]?.source else { throw Failure.closed }
    let result = try await run(sourceId: sourceId) {
      try await Self.transcribe(source: source, locale: locale)
    }
    guard leases[sourceId] != nil, !closing.contains(sourceId) else { throw Failure.closed }
    return result
  }

  /// Cancel work without losing the original, so a review can retry or undo.
  func cancel(_ sourceId: String) async {
    if var lease = leases[sourceId] {
      lease.completed = nil
      if lease.revision < UInt64.max { lease.revision += 1 }
      leases[sourceId] = lease
    }
    let pending = jobs.values.filter { $0.sourceId == sourceId }
    pending.forEach { $0.cancel() }
    for job in pending { await job.drain() }
  }

  func release(_ sourceId: String) async throws {
    guard let lease = leases[sourceId] else { return }
    closing.insert(sourceId)
    await cancel(sourceId)
    do { try FileManager.default.removeItem(at: lease.root) }
    catch let error as CocoaError where error.code == .fileNoSuchFile || error.code == .fileReadNoSuchFile { }
    leases.removeValue(forKey: sourceId)
    closing.remove(sourceId)
  }

  func releaseOutput(sourceId: String, uri: String) throws {
    guard var lease = leases[sourceId], let url = URL(string: uri), url.isFileURL,
          url.standardizedFileURL.deletingLastPathComponent() == lease.root,
          UUID(uuidString: url.deletingPathExtension().lastPathComponent) != nil,
          url.pathExtension == "m4a" else { return }
    if lease.completed?.uri == uri {
      lease.completed = nil
      leases[sourceId] = lease
    }
    do { try FileManager.default.removeItem(at: url) }
    catch let error as CocoaError where error.code == .fileNoSuchFile || error.code == .fileReadNoSuchFile { }
  }

  /// Only the latest completed export of a live original lease can be sent.
  /// A retained file after a failed cleanup is not a send capability.
  func admitSend(sourceId: String, uri: String) throws -> Render {
    guard !closing.contains(sourceId), let render = leases[sourceId]?.completed,
          render.uri == uri else { throw Failure.closed }
    return render
  }

  func admitSource(sourceId: String, uri: String) throws -> Source {
    guard !closing.contains(sourceId), let source = leases[sourceId]?.source,
          source.uri == uri else { throw Failure.closed }
    return source
  }

  func releaseAll() async throws {
    let ids = Array(leases.keys)
    var failed = false
    for id in ids { do { try await release(id) } catch { failed = true } }
    if failed { throw Failure.renderFailed }
  }

  static func validate(ranges: [Range], source: Source) throws -> Int64 {
    guard !ranges.isEmpty, ranges.count <= 4_096 else { throw Failure.invalidRanges }
    var previous: Int64 = 0
    var count: Int64 = 0
    for range in ranges {
      guard range.startFrame >= previous, range.endFrame > range.startFrame, range.endFrame <= source.frameCount else { throw Failure.invalidRanges }
      count += range.endFrame - range.startFrame
      previous = range.endFrame
    }
    guard Double(count) / source.sampleRate >= 0.25 else { throw Failure.invalidRanges }
    return count
  }

  private static func selection(_ uri: String) throws -> URL {
    guard let url = URL(string: uri), url.isFileURL else { throw Failure.invalidAudio }
    let resolved = url.resolvingSymlinksInPath().standardizedFileURL
    let roots = [URL.cachesDirectory, URL.temporaryDirectory].map { $0.resolvingSymlinksInPath().standardizedFileURL.path + "/" }
    guard roots.contains(where: resolved.path.hasPrefix), !resolved.pathExtension.isEmpty else { throw Failure.invalidAudio }
    let values = try resolved.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
    guard values.isRegularFile == true, let size = values.fileSize, size > 0, UInt64(size) <= ZoenMediaFiles.maximumBytes else { throw Failure.invalidAudio }
    return resolved
  }

  /// Decoded PCM stays in bounded native buffers; JS receives only 160 peaks.
  private static func waveform(_ file: AVAudioFile) throws -> [Double] {
    let count = min(160, Int(file.length))
    guard count > 0, let buffer = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: 8_192) else { throw Failure.invalidAudio }
    var levels = [Double](repeating: 0, count: count)
    file.framePosition = 0
    while file.framePosition < file.length {
      try Task.checkCancellation()
      let offset = file.framePosition
      try file.read(into: buffer, frameCount: AVAudioFrameCount(min(8_192, file.length - offset)))
      guard buffer.frameLength > 0, let data = buffer.floatChannelData else { throw Failure.invalidAudio }
      for channel in 0..<Int(buffer.format.channelCount) {
        for frame in 0..<Int(buffer.frameLength) {
          let value = Double(abs(data[channel][frame]))
          guard value.isFinite else { throw Failure.invalidAudio }
          let bin = min(count - 1, Int((offset + Int64(frame)) * Int64(count) / file.length))
          levels[bin] = max(levels[bin], min(1, value))
        }
      }
    }
    return levels
  }

  private static func render(source: Source, ranges: [Range], output: URL) throws {
    guard let input = URL(string: source.uri) else { throw Failure.invalidAudio }
    let file = try AVAudioFile(forReading: input, commonFormat: .pcmFormatFloat32, interleaved: false)
    guard file.length == source.frameCount, file.processingFormat.sampleRate == source.sampleRate,
          let buffer = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: 8_192) else { throw Failure.invalidAudio }
    var writer: AVAudioFile? = try AVAudioFile(forWriting: output, settings: [
      AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: source.sampleRate,
      AVNumberOfChannelsKey: file.processingFormat.channelCount, AVEncoderBitRateKey: 128_000,
    ], commonFormat: .pcmFormatFloat32, interleaved: false)
    defer { writer = nil }
    for range in ranges {
      file.framePosition = range.startFrame
      let length = range.endFrame - range.startFrame
      let fade = min(Int64((source.sampleRate * 0.015).rounded()), length / 2)
      while file.framePosition < range.endFrame {
        try Task.checkCancellation()
        let offset = file.framePosition - range.startFrame
        try file.read(into: buffer, frameCount: AVAudioFrameCount(min(8_192, range.endFrame - file.framePosition)))
        guard buffer.frameLength > 0, let data = buffer.floatChannelData else { throw Failure.renderFailed }
        for frame in 0..<Int(buffer.frameLength) {
          let position = offset + Int64(frame)
          let gain = fade > 0 ? min(1, Double(min(position, length - 1 - position)) / Double(fade)) : 1
          for channel in 0..<Int(buffer.format.channelCount) { data[channel][frame] *= Float(gain) }
        }
        try writer?.write(from: buffer)
      }
    }
    // AVAudioFile must close before another reader observes the AAC packet table.
    writer = nil
    try FileManager.default.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: output.path)
  }

  /// Speech time-codes are external floating-point metadata. Clamp in seconds
  /// before multiplying or converting, including finite values outside Int64.
  /// Keep each attributed fragment intact; no per-word timing is synthesized.
  static func speechRange(startSeconds: Double, endSeconds: Double, source: Source, previousEnd: Int64) -> Range? {
    guard startSeconds.isFinite, endSeconds.isFinite, endSeconds > startSeconds,
          source.sampleRate.isFinite, source.sampleRate > 0,
          source.frameCount > 0, source.frameCount <= 9_007_199_254_740_991,
          previousEnd >= 0, previousEnd <= source.frameCount else { return nil }
    let duration = Double(source.frameCount) / source.sampleRate
    let start = max(0, min(duration, startSeconds)) * source.sampleRate
    let end = max(0, min(duration, endSeconds)) * source.sampleRate
    let a = max(previousEnd, min(source.frameCount, Int64(start.rounded(.down))))
    let b = max(a, min(source.frameCount, Int64(end.rounded(.up))))
    return b > a ? Range(startFrame: a, endFrame: b) : nil
  }

  /// Local file analysis need not run in real time. Give longer recordings a
  /// proportionate budget (30s + half their length), bounded to 1–15 minutes.
  /// This timer starts after any explicit language-model installation finishes.
  static func processingTimeout(durationSeconds: Double) -> Double {
    max(60, min(900, 30 + durationSeconds / 2))
  }

  private static func transcribe(source: Source, locale: String) async throws -> Transcript {
    guard SpeechTranscriber.isAvailable,
          let supported = await SpeechTranscriber.supportedLocale(equivalentTo: Locale(identifier: locale == "pt-BR" ? "pt-BR" : "en-US")),
          let url = URL(string: source.uri) else { return .unavailable }
    let transcriber = SpeechTranscriber(locale: supported, transcriptionOptions: [], reportingOptions: [], attributeOptions: [.audioTimeRange])
    try Task.checkCancellation()
    if let installation = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
      try await withTaskCancellationHandler {
        try await installation.downloadAndInstall()
      } onCancel: { installation.progress.cancel() }
    }
    try Task.checkCancellation()
    let analyzer = SpeechAnalyzer(modules: [transcriber])
    return try await withTaskCancellationHandler {
      do {
        return try await withThrowingTaskGroup(of: Transcript.self) { group in
          group.addTask {
            async let processing: Void = analyzer.start(inputAudioFile: AVAudioFile(forReading: url), finishAfterFile: true)
            var text = ""
            var words: [Word] = []
            for try await result in transcriber.results {
              try Task.checkCancellation()
              for run in result.text.runs {
                let fragment = String(result.text[run.range].characters)
                text += fragment
                guard let timing = run.audioTimeRange else { continue }
                // Adjacent Speech times may straddle one PCM frame after
                // quantization. Keep word ranges monotonic in original frames.
                guard let range = speechRange(startSeconds: timing.start.seconds,
                  endSeconds: CMTimeRangeGetEnd(timing).seconds, source: source,
                  previousEnd: words.last?.endFrame ?? 0) else { continue }
                let word = fragment.trimmingCharacters(in: .whitespacesAndNewlines)
                if !word.isEmpty {
                  words.append(Word(id: "\(source.id):\(words.count)", text: word, startFrame: range.startFrame, endFrame: range.endFrame))
                }
              }
              guard text.utf8.count <= 65_536, words.count <= 10_000 else { throw Failure.invalidAudio }
            }
            try await processing
            return Transcript(status: "ready", text: text, words: words)
          }
          group.addTask {
            try await Task.sleep(for: .seconds(processingTimeout(durationSeconds: source.durationMs / 1_000)))
            await analyzer.cancelAndFinishNow()
            return .unavailable
          }
          defer { group.cancelAll() }
          guard let result = try await group.next() else { return .unavailable }
          await analyzer.cancelAndFinishNow()
          return result
        }
      } catch {
        await analyzer.cancelAndFinishNow()
        if Task.isCancelled { throw CancellationError() }
        return .unavailable
      }
    } onCancel: { Task { await analyzer.cancelAndFinishNow() } }
  }
}
