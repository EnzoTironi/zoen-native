import AVFoundation
import CoreGraphics
import Foundation

/// A disposable first-frame display request over an already-authorized local
/// file. It never copies media, writes a cache, or acquires file authority.
@MainActor final class ZoenAttachmentVideoThumbnail {
  private var generator: AVAssetImageGenerator?
  private var task: Task<Void, Never>?
  private var generation = UUID()

  enum Failure: Error { case invalidLocalRequest }

  func load(url: URL, maximumPixels: CGFloat,
            completion: @escaping (Result<CGImage, Error>) -> Void) {
    cancel()
    guard url.isFileURL, url.query == nil, url.fragment == nil,
          maximumPixels.isFinite, maximumPixels > 0 else {
      completion(.failure(Failure.invalidLocalRequest)); return
    }
    let ticket = generation
    // A local reference movie must not cause reads outside this exact leased
    // container, whether its secondary reference is local or remote.
    let asset = AVURLAsset(url: url, options: [AVURLAssetReferenceRestrictionsKey:
      NSNumber(value: AVAssetReferenceRestrictions.forbidAll.rawValue)])
    let generator = AVAssetImageGenerator(asset: asset)
    generator.appliesPreferredTrackTransform = true
    let pixels = min(2048, maximumPixels)
    generator.maximumSize = CGSize(width: pixels, height: pixels)
    generator.requestedTimeToleranceBefore = .zero
    generator.requestedTimeToleranceAfter = .zero
    self.generator = generator
    task = Task { @MainActor [weak self] in
      do {
        try Task.checkCancellation()
        let (image, _) = try await generator.image(at: .zero)
        guard let self, self.generation == ticket, !Task.isCancelled else { return }
        self.task = nil; self.generator = nil
        completion(.success(image))
      } catch {
        guard let self, self.generation == ticket, !Task.isCancelled else { return }
        self.task = nil; self.generator = nil
        completion(.failure(error))
      }
    }
  }

  func cancel() {
    generation = UUID()
    task?.cancel(); task = nil
    generator?.cancelAllCGImageGeneration(); generator = nil
  }

  deinit {
    task?.cancel()
    generator?.cancelAllCGImageGeneration()
  }
}
