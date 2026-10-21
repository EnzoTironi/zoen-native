import AVFoundation
import CoreGraphics
import CryptoKit
import XCTest
@testable import ZoenCore

/// Executes the production thumbnail owner against actual local H.264 files.
/// This does not prove gallery layout, iOS playback or bridge authorization.
@MainActor final class AttachmentLocalVideoThumbnailTests: XCTestCase {
  private func fixture(blue: Bool = false, rotated: Bool = false) async throws -> URL {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    let url = directory.appendingPathComponent("fixture.mp4")
    let writer = try AVAssetWriter(outputURL: url, fileType: .mp4)
    let input = AVAssetWriterInput(mediaType: .video, outputSettings: [
      AVVideoCodecKey: AVVideoCodecType.h264, AVVideoWidthKey: 80, AVVideoHeightKey: 48,
    ])
    if rotated { input.transform = CGAffineTransform(rotationAngle: .pi / 2) }
    let adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input,
      sourcePixelBufferAttributes: [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
        kCVPixelBufferWidthKey as String: 80, kCVPixelBufferHeightKey as String: 48])
    writer.add(input)
    guard writer.startWriting() else { throw writer.error ?? CocoaError(.fileWriteUnknown) }
    writer.startSession(atSourceTime: .zero)
    for frame in 0..<8 {
      var attempts = 0
      while !input.isReadyForMoreMediaData {
        attempts += 1
        guard attempts < 2000 else { throw CocoaError(.fileWriteUnknown) }
        try await Task.sleep(for: .milliseconds(1))
      }
      var pixel: CVPixelBuffer?
      guard CVPixelBufferCreate(kCFAllocatorDefault, 80, 48, kCVPixelFormatType_32BGRA,
        nil, &pixel) == kCVReturnSuccess, let pixel else { throw CocoaError(.fileWriteUnknown) }
      CVPixelBufferLockBaseAddress(pixel, [])
      let stride = CVPixelBufferGetBytesPerRow(pixel) / MemoryLayout<UInt32>.size
      let base = CVPixelBufferGetBaseAddress(pixel)!.assumingMemoryBound(to: UInt32.self)
      for row in 0..<48 {
        for column in 0..<80 { base[row * stride + column] = blue || frame > 0 ? 0xff0000ff : 0xffff0000 }
      }
      CVPixelBufferUnlockBaseAddress(pixel, [])
      guard adaptor.append(pixel, withPresentationTime: CMTime(value: Int64(frame), timescale: 30)) else {
        throw writer.error ?? CocoaError(.fileWriteUnknown)
      }
    }
    input.markAsFinished()
    await writer.finishWriting()
    guard writer.status == .completed else { throw writer.error ?? CocoaError(.fileWriteUnknown) }
    return url
  }
  private func image(_ owner: ZoenAttachmentVideoThumbnail, _ url: URL, pixels: CGFloat = 64) async throws -> CGImage {
    try await withCheckedThrowingContinuation { continuation in
      owner.load(url: url, maximumPixels: pixels) { continuation.resume(with: $0) }
    }
  }
  private func rgb(_ image: CGImage) -> [UInt8] {
    var bytes = [UInt8](repeating: 0, count: 4)
    bytes.withUnsafeMutableBytes { buffer in
      let context = CGContext(data: buffer.baseAddress, width: 1, height: 1, bitsPerComponent: 8,
        bytesPerRow: 4, space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
      context.draw(image, in: CGRect(x: 0, y: 0, width: 1, height: 1))
    }
    return bytes
  }
  private func remove(_ url: URL) { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }

  func testActualFirstFrameIsBoundedAndDoesNotCopyOrChangeTheAuthorizedFile() async throws {
    let url = try await fixture(); defer { remove(url) }
    let before = try Data(contentsOf: url)
    let owner = ZoenAttachmentVideoThumbnail()
    let frame = try await image(owner, url)
    XCTAssertLessThanOrEqual(max(frame.width, frame.height), 64)
    let color = rgb(frame)
    XCTAssertGreaterThan(color[0], 200, "The first frame is red; later frames are blue")
    XCTAssertLessThan(color[2], 40)
    XCTAssertEqual(SHA256.hash(data: before), SHA256.hash(data: try Data(contentsOf: url)))
    XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: url.deletingLastPathComponent().path), ["fixture.mp4"])
  }

  func testTrackTransformProducesTheActualPortraitThumbnail() async throws {
    let url = try await fixture(rotated: true); defer { remove(url) }
    let frame = try await image(ZoenAttachmentVideoThumbnail(), url)
    XCTAssertGreaterThan(frame.height, frame.width)
    XCTAssertLessThanOrEqual(max(frame.width, frame.height), 64)
  }

  func testReplacingARequestOnlyDeliversTheNewItem() async throws {
    let red = try await fixture(); defer { remove(red) }
    let blue = try await fixture(blue: true); defer { remove(blue) }
    let owner = ZoenAttachmentVideoThumbnail()
    let stale = expectation(description: "replaced item is not delivered"); stale.isInverted = true
    owner.load(url: red, maximumPixels: 64) { _ in stale.fulfill() }
    let frame = try await image(owner, blue)
    XCTAssertGreaterThan(rgb(frame)[2], 200)
    await fulfillment(of: [stale], timeout: 0.1)
  }

  func testCancelledAndReleasedOwnersNeverDeliverToARetiredSurface() async throws {
    let url = try await fixture(); defer { remove(url) }
    let cancelled = expectation(description: "cancelled result"); cancelled.isInverted = true
    let owner = ZoenAttachmentVideoThumbnail()
    owner.load(url: url, maximumPixels: 64) { _ in cancelled.fulfill() }
    owner.cancel()
    let released = expectation(description: "released owner result"); released.isInverted = true
    var transient: ZoenAttachmentVideoThumbnail? = ZoenAttachmentVideoThumbnail()
    weak var weakOwner = transient
    transient?.load(url: url, maximumPixels: 64) { _ in released.fulfill() }
    transient = nil
    XCTAssertNil(weakOwner, "A decoding task must not retain its display owner")
    await fulfillment(of: [cancelled, released], timeout: 0.1)
  }

  func testMalformedLocalVideoCanFailThenRetryWithoutPoisoningTheOwner() async throws {
    let good = try await fixture(); defer { remove(good) }
    let broken = good.deletingLastPathComponent().appendingPathComponent("broken.mp4")
    try Data("not a video".utf8).write(to: broken)
    let owner = ZoenAttachmentVideoThumbnail()
    do { _ = try await image(owner, broken); XCTFail("Malformed media must fail") } catch {}
    let frame = try await image(owner, good)
    XCTAssertGreaterThan(rgb(frame)[0], 200)
  }

  func testNonlocalAndInvalidSizeRequestsAreRejectedBeforeDecoding() async throws {
    let owner = ZoenAttachmentVideoThumbnail()
    for (url, pixels) in [(URL(string: "https://invalid.example/video.mp4")!, CGFloat(64)),
      (URL(fileURLWithPath: "/unread.mp4"), CGFloat.nan),
      (URL(fileURLWithPath: "/unread.mp4"), CGFloat.zero)] {
      do { _ = try await image(owner, url, pixels: pixels); XCTFail("Invalid request must fail") }
      catch ZoenAttachmentVideoThumbnail.Failure.invalidLocalRequest {} catch { XCTFail("Unexpected error: \(error)") }
    }
  }
}
