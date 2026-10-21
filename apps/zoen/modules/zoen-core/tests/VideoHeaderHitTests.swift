import XCTest
import UIKit
@testable import ZoenCore

@MainActor private final class VideoHeaderSource: NSObject, ZoenTGMediaPickerGalleryVideoScrubberDataSource {
  func videoScrubberDuration(_ scrubber: ZoenTGMediaPickerGalleryVideoScrubber!) -> TimeInterval { 10 }
  func videoScrubber(_ scrubber: ZoenTGMediaPickerGalleryVideoScrubber!, evenlySpacedTimestamps count: Int,
                     startingAt start: TimeInterval, endingAt end: TimeInterval) -> [Any]! { [] }
  func videoScrubber(_ scrubber: ZoenTGMediaPickerGalleryVideoScrubber!, requestThumbnailImagesForTimestamps timestamps: [Any]!,
                     size: CGSize, isSummaryThumbnails summary: Bool) {}
  func videoScrubberThumbnailAspectRatio(_ scrubber: ZoenTGMediaPickerGalleryVideoScrubber!) -> CGFloat { 1 }
  func videoScrubberOriginalSize(_ scrubber: ZoenTGMediaPickerGalleryVideoScrubber!, cropRect: UnsafeMutablePointer<CGRect>!,
                                cropOrientation: UnsafeMutablePointer<UIImage.Orientation>!, cropMirrored: UnsafeMutablePointer<Bool>!) -> CGSize {
    cropRect?.pointee = CGRect(x: 0, y: 0, width: 100, height: 100)
    cropOrientation?.pointee = .up; cropMirrored?.pointee = false
    return CGSize(width: 100, height: 100)
  }
}

// Actual UIKit+original ObjC hitTest regression in an isolated iOS XCTest target.
// The data source supplies fixed metadata/no thumbnails;
// the hierarchy, controls and hitTest implementations are the production ones.
@MainActor final class VideoHeaderHitTests: XCTestCase {
  private struct Fixture {
    let root: UIView
    let header: ZoenTGMediaPickerScrubberHeaderView
    let scrubber: ZoenTGMediaPickerGalleryVideoScrubber
    let toolbar: UIView
    let source: VideoHeaderSource
    let handles: [UIButton]
  }
  private func descendants(_ view: UIView) -> [UIView] {
    [view] + view.subviews.flatMap(descendants)
  }
  private func fixture(size: CGSize, safeArea: UIEdgeInsets, left: Bool) throws -> Fixture {
    let root = UIView(frame: CGRect(origin: .zero, size: size))
    // This is the exact production factory and original scrubber hierarchy.
    let scrubber = try XCTUnwrap(ZoenTGMediaPickerGalleryVideoScrubber(frame: .zero, cover: false))
    let header = ZoenTelegramVideoEditorLayout.makeHeader(scrubber: scrubber)
    let layout = ZoenTelegramVideoEditorLayout.make(bounds: root.bounds, safeArea: safeArea, landscapeLeft: left)
    header.frame = layout.header; header.safeAreaInset = layout.headerSafeArea
    root.addSubview(header)
    let toolbar = UIView(frame: layout.toolbar)
    let save = UIButton(frame: layout.commit), cancel = UIButton(frame: layout.cancel)
    toolbar.addSubview(save); toolbar.addSubview(cancel); root.addSubview(toolbar)
    let source = VideoHeaderSource()
    scrubber.dataSource = source; scrubber.allowsTrimming = true
    // The production owner sets its valid initial range before reloadDataAndReset(false).
    // Leaving both endpoints at zero overlaps the original handles and tests the wrong state.
    scrubber.trimStartValue = 0
    scrubber.trimEndValue = source.videoScrubberDuration(scrubber)
    root.layoutIfNeeded(); header.layoutIfNeeded(); header.panelView.layoutIfNeeded()
    scrubber.reloadDataAndReset(false)
    scrubber.setValue(5, resetPosition: true) // keep the seek cursor clear of both trim handles
    scrubber.setNeedsLayout(); scrubber.layoutIfNeeded()
    let trim = try XCTUnwrap(descendants(scrubber).first { $0 is ZoenTGMediaPickerGalleryVideoTrimView })
    trim.setNeedsLayout(); trim.layoutIfNeeded()
    let handles = trim.subviews.compactMap { $0 as? UIButton }.sorted { $0.frame.minX < $1.frame.minX }
    XCTAssertEqual(handles.count, 2)
    XCTAssertLessThan(handles[0].frame.maxX, handles[1].frame.minX, "The valid initial trim range must separate both original handles")
    return Fixture(root: root, header: header, scrubber: scrubber, toolbar: toolbar, source: source, handles: handles)
  }
  func testTopCenterAndBottomOfBothTrimHandlesReachTheOriginalButtonThroughEveryParent() throws {
    let cases: [(CGSize, UIEdgeInsets, Bool)] = [
      (CGSize(width: 402, height: 874), UIEdgeInsets(top: 62, left: 0, bottom: 34, right: 0), false),
      (CGSize(width: 390, height: 844), UIEdgeInsets(top: 47, left: 0, bottom: 34, right: 0), false),
      (CGSize(width: 375, height: 667), .zero, false),
      (CGSize(width: 874, height: 402), UIEdgeInsets(top: 0, left: 62, bottom: 21, right: 0), true),
      (CGSize(width: 874, height: 402), UIEdgeInsets(top: 0, left: 0, bottom: 21, right: 62), false),
    ]
    for (size, safeArea, left) in cases {
      let f = try fixture(size: size, safeArea: safeArea, left: left)
      XCTAssertEqual(f.scrubber.frame.minY, 0)
      XCTAssertEqual(f.header.panelView.frame.height, 64)
      for handle in f.handles {
        for fraction in [CGFloat(0.02), 0.5, 0.98] {
          let point = handle.convert(CGPoint(x: handle.bounds.midX, y: handle.bounds.height * fraction), to: f.root)
          let hit = try XCTUnwrap(f.root.hitTest(point, with: nil), "Missing hit at \(point), \(size)")
          XCTAssertTrue(hit === handle || hit.isDescendant(of: handle), "Parent clipped a visible trim point at \(point), \(size); hit \(hit)")
          XCTAssertFalse(f.toolbar.frame.contains(point), "A trim gesture must never land on Save/Cancel")
        }
      }
    }
  }
  func testRotationRelayoutUsesCurrentWidthAndConsumesOnlyTheToolbarSideSafeInset() {
    let safe = UIEdgeInsets(top: 0, left: 62, bottom: 21, right: 11)
    let left = ZoenTelegramVideoEditorLayout.make(bounds: CGRect(x: 0, y: 0, width: 874, height: 402), safeArea: safe, landscapeLeft: true)
    XCTAssertEqual(left.header.minX, 106)
    XCTAssertEqual(left.header.width, 768)
    XCTAssertEqual(left.header.height, 64)
    XCTAssertEqual(left.headerSafeArea.left, 0)
    XCTAssertEqual(left.headerSafeArea.right, 11)
    let portrait = ZoenTelegramVideoEditorLayout.make(bounds: CGRect(x: 0, y: 0, width: 402, height: 874), safeArea: UIEdgeInsets(top: 62, left: 0, bottom: 34, right: 0), landscapeLeft: false)
    XCTAssertEqual(portrait.header, CGRect(x: 0, y: 62, width: 402, height: 72))
  }
  func testAHeaderRetiredByItsCurrentOwnerCannotReturnAHiddenTrimTarget() throws {
    let f = try fixture(size: CGSize(width: 402, height: 874), safeArea: UIEdgeInsets(top: 62, left: 0, bottom: 34, right: 0), left: false)
    let handle = try XCTUnwrap(f.handles.first)
    let point = handle.convert(CGPoint(x: handle.bounds.midX, y: handle.bounds.midY), to: f.root)
    f.header.isUserInteractionEnabled = false
    XCTAssertFalse(f.root.hitTest(point, with: nil)?.isDescendant(of: handle) == true)
    f.header.isUserInteractionEnabled = true; f.header.isHidden = true
    XCTAssertFalse(f.root.hitTest(point, with: nil)?.isDescendant(of: handle) == true)
  }
}
