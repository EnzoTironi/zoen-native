import XCTest
import CoreGraphics
@testable import ZoenCore

/// Pure original geometry + bounded Expo adapter; this does not qualify image
/// decoding, Fabric layout, physical gestures or a rendered iPhone album.
final class AttachmentMosaicTests: XCTestCase {
  func testOriginalMosaicKeepsEveryOrderedFrameInsideBoundsWithoutOverlap() throws {
    for count in 2...10 {
      let sizes = (0..<count).map { index in [Double(100 + index * 13), Double(80 + (index % 3) * 100)] }
      let result = try XCTUnwrap(ZoenAttachmentMosaicBridge.layout(width: 294, height: 294, scale: 3, sizes: sizes))
      let width = try XCTUnwrap(result["width"] as? CGFloat), height = try XCTUnwrap(result["height"] as? CGFloat)
      let items = try XCTUnwrap(result["items"] as? [[String: Any]])
      XCTAssertEqual(items.count, count)
      var frames: [CGRect] = []
      for item in items {
        let frame = CGRect(x: try XCTUnwrap(item["x"] as? CGFloat), y: try XCTUnwrap(item["y"] as? CGFloat),
          width: try XCTUnwrap(item["width"] as? CGFloat), height: try XCTUnwrap(item["height"] as? CGFloat))
        XCTAssertGreaterThan(frame.width, 0); XCTAssertGreaterThan(frame.height, 0)
        XCTAssertGreaterThanOrEqual(frame.minX, 0); XCTAssertGreaterThanOrEqual(frame.minY, 0)
        XCTAssertLessThanOrEqual(frame.maxX, width + 1); XCTAssertLessThanOrEqual(frame.maxY, height + 1)
        XCTAssertTrue(frames.allSatisfy { !$0.intersects(frame) })
        frames.append(frame)
      }
    }
  }
  func testBridgeRejectsUnboundedNonPixelAndMalformedInputs() {
    let valid = [[100.0, 200.0], [200.0, 100.0]]
    for sizes in [[], [[100, 200]], Array(repeating: [100, 100], count: 11), [[0, 1], [1, 1]], [[Double.leastNonzeroMagnitude, 1], [1, 1]], [[Double.infinity, 1], [1, 1]], [[100_001, 1], [1, 1]], [[1], [1, 1]]] {
      XCTAssertNil(ZoenAttachmentMosaicBridge.layout(width: 294, height: 294, scale: 3, sizes: sizes))
    }
    for invalid in [Double.nan, .infinity, -1, 0, 4097] {
      XCTAssertNil(ZoenAttachmentMosaicBridge.layout(width: invalid, height: 294, scale: 3, sizes: valid))
      XCTAssertNil(ZoenAttachmentMosaicBridge.layout(width: 294, height: invalid, scale: 3, sizes: valid))
    }
    for invalid in [Double.nan, .infinity, 0, 9] {
      XCTAssertNil(ZoenAttachmentMosaicBridge.layout(width: 294, height: 294, scale: invalid, sizes: valid))
    }
  }
}
