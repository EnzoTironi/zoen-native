import AVFoundation
import Foundation

/// A stateless native renderer for the original gallery's trim/sendAsGif
/// operations. The canonical capture owner allocates and registers destination
/// BEFORE calling this function, owns every partial output and cleans it after
/// cancellation/failure. No temporary directory or second file owner is created.
enum ZoenTelegramVideoRenderer {
  struct Result: Sendable {
    let duration: CMTime
    let dimensions: CGSize
    let bytes: UInt64
    let hasAudio: Bool
  }

  static func render(asset: AVAsset, range: CMTimeRange, sendAsGif: Bool,
                     destination: URL, maximumBytes: UInt64,
                     progress: (@Sendable (Double) async -> Void)? = nil) async throws -> Result {
    try Task.checkCancellation()
    guard destination.isFileURL, destination.query == nil, destination.fragment == nil,
          destination.pathExtension.lowercased() == "mp4", maximumBytes > 0,
          !FileManager.default.fileExists(atPath: destination.path) else {
      throw ZoenTelegramVideoFailure.destination
    }
    let parent = try FileManager.default.attributesOfItem(atPath: destination.deletingLastPathComponent().path)
    guard parent[.type] as? FileAttributeType == .typeDirectory else { throw ZoenTelegramVideoFailure.destination }
    let duration = try await asset.load(.duration)
    try validate(range: range, duration: duration)
    // The gallery represents seconds at nanosecond precision. Clamp only the
    // sub-nanosecond rounding of an unchanged endpoint back to the exact asset.
    let range = CMTimeRange(start: range.start, end: CMTimeMinimum(range.end, duration))
    let videos = try await asset.loadTracks(withMediaType: .video)
    guard let video = videos.first else { throw ZoenTelegramVideoFailure.unavailable }
    let composition = AVMutableComposition()
    guard let outputVideo = composition.addMutableTrack(withMediaType: .video, preferredTrackID: kCMPersistentTrackID_Invalid) else {
      throw ZoenTelegramVideoFailure.export
    }
    try outputVideo.insertTimeRange(range, of: video, at: .zero)
    outputVideo.preferredTransform = try await video.load(.preferredTransform)
    if !sendAsGif {
      for audio in try await asset.loadTracks(withMediaType: .audio) {
        let available = CMTimeRangeGetIntersection(range, otherRange: try await audio.load(.timeRange))
        guard available.isValid, available.duration.seconds > 0 else { continue }
        guard let output = composition.addMutableTrack(withMediaType: .audio, preferredTrackID: kCMPersistentTrackID_Invalid) else {
          throw ZoenTelegramVideoFailure.export
        }
        try output.insertTimeRange(available, of: audio, at: CMTimeSubtract(available.start, range.start))
      }
    }
    // Native AVFoundation encodes exact selected frames; no filter, sticker,
    // Telegram bitrate/preset policy or hidden lower-quality fallback is added.
    guard let session = AVAssetExportSession(asset: composition, presetName: AVAssetExportPresetHighestQuality) else {
      throw ZoenTelegramVideoFailure.export
    }
    session.shouldOptimizeForNetworkUse = true
    let monitor = Task {
      while !Task.isCancelled {
        if let size = (try? FileManager.default.attributesOfItem(atPath: destination.path)[.size]) as? NSNumber,
           size.uint64Value > maximumBytes { session.cancelExport(); return }
        if let progress { await progress(Double(session.progress)) }
        do { try await Task.sleep(for: .milliseconds(100)) } catch { return }
      }
    }
    defer { monitor.cancel() }
    do {
      try await withTaskCancellationHandler {
        try await session.export(to: destination, as: .mp4)
      } onCancel: { session.cancelExport() }
    } catch {
      try Task.checkCancellation()
      if let size = (try? FileManager.default.attributesOfItem(atPath: destination.path)[.size]) as? NSNumber,
         size.uint64Value > maximumBytes { throw ZoenTelegramVideoFailure.tooLarge }
      throw error
    }
    try Task.checkCancellation()
    let attributes = try FileManager.default.attributesOfItem(atPath: destination.path)
    guard attributes[.type] as? FileAttributeType == .typeRegular,
          let size = attributes[.size] as? NSNumber, size.uint64Value > 0 else {
      throw ZoenTelegramVideoFailure.export
    }
    guard size.uint64Value <= maximumBytes else { throw ZoenTelegramVideoFailure.tooLarge }
    let rendered = AVURLAsset(url: destination)
    let actualDuration = try await rendered.load(.duration)
    let tracks = try await rendered.loadTracks(withMediaType: .video)
    let audio = try await rendered.loadTracks(withMediaType: .audio)
    guard actualDuration.seconds.isFinite, actualDuration.seconds > 0, let track = tracks.first,
          !sendAsGif || audio.isEmpty else { throw ZoenTelegramVideoFailure.export }
    let natural = try await track.load(.naturalSize)
    let transform = try await track.load(.preferredTransform)
    let sizeInDisplay = natural.applying(transform)
    if let progress { await progress(1) }
    return Result(duration: actualDuration, dimensions: CGSize(width: abs(sizeInDisplay.width), height: abs(sizeInDisplay.height)),
                  bytes: size.uint64Value, hasAudio: !audio.isEmpty)
  }

  static func validate(range: CMTimeRange, duration: CMTime) throws {
    guard range.isValid, !range.isEmpty, !range.isIndefinite,
          range.start.seconds.isFinite, range.duration.seconds.isFinite,
          duration.seconds.isFinite, duration.seconds > 0,
          CMTimeCompare(range.start, .zero) >= 0, CMTimeCompare(range.start, duration) < 0,
          CMTimeCompare(range.duration, .zero) > 0,
          range.end.seconds - duration.seconds <= 0.000_000_001 else { throw ZoenTelegramVideoFailure.invalidRange }
  }
}
