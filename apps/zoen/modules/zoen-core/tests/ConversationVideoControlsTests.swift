import AVFoundation
import UIKit
import XCTest

@MainActor final class ConversationVideoControlsTests: XCTestCase {
  private func descendants(_ view: UIView) -> [UIView] { [view] + view.subviews.flatMap(descendants) }
  private func control(_ view: UIView, _ id: String) throws -> UIControl {
    try XCTUnwrap(descendants(view).first { $0.accessibilityIdentifier == id } as? UIControl)
  }
  private func make() -> (ZoenConversationVideoPlayback, ZoenConversationVideoControls) {
    let player = AVPlayer(playerItem: AVPlayerItem(asset: AVMutableComposition()))
    let playback = ZoenConversationVideoPlayback(player: player)
    playback.setActive(true)
    return (playback, ZoenConversationVideoControls(playback: playback))
  }
  func testOriginalViewerTargetsAndFooterRespectBothSafeAreasAndCaption() throws {
    let (playback, view) = make()
    defer { playback.retire(); view.retire() }
    for (size, safe, caption) in [
      (CGSize(width: 390, height: 844), UIEdgeInsets(top: 59, left: 0, bottom: 34, right: 0), CGRect(x: 16, y: 730, width: 358, height: 40)),
      (CGSize(width: 844, height: 390), UIEdgeInsets(top: 0, left: 59, bottom: 21, right: 59), CGRect(x: 75, y: 300, width: 694, height: 32))
    ] {
      view.frame = CGRect(origin: .zero, size: size)
      view.updatePresentation(active: true, chromeVisible: true, safeArea: safe, captionFrame: caption)
      view.layoutIfNeeded()
      let play = try control(view, "conversation-video-play-pause")
      let back = try control(view, "conversation-video-seek-backward")
      let forward = try control(view, "conversation-video-seek-forward")
      XCTAssertEqual(play.bounds.size, CGSize(width: 92, height: 92))
      XCTAssertEqual(back.bounds.size, CGSize(width: 64, height: 64))
      XCTAssertEqual(forward.bounds.size, CGSize(width: 64, height: 64))
      XCTAssertEqual(play.frame.minX - back.frame.maxX, 30)
      XCTAssertEqual(forward.frame.minX - play.frame.maxX, 30)
      XCTAssertEqual(view.scrubber.bounds.height, 44)
      XCTAssertLessThanOrEqual(view.scrubber.frame.maxY, caption.minY - 10)
      XCTAssertGreaterThanOrEqual(view.scrubber.frame.minX, safe.left)
      XCTAssertLessThanOrEqual(view.scrubber.frame.maxX, size.width - safe.right)
      XCTAssertTrue(descendants(view).allSatisfy { $0.accessibilityIdentifier?.contains("close") != true })
      XCTAssertTrue(view.hitTest(CGPoint(x: 2, y: 2), with: nil) == nil)
    }
  }
  func testActualControlDispatchStopsOnHiddenInactiveAndRetiredOwners() throws {
    let (playback, view) = make()
    defer { playback.retire() }
    view.frame = CGRect(x: 0, y: 0, width: 390, height: 844)
    view.updatePresentation(active: true, chromeVisible: true, safeArea: .zero, captionFrame: .zero)
    let play = try control(view, "conversation-video-play-pause")
    play.sendActions(for: .touchUpInside)
    XCTAssertTrue(playback.state.playing)
    play.sendActions(for: .touchUpInside)
    XCTAssertFalse(playback.state.playing)
    view.updatePresentation(active: true, chromeVisible: false, safeArea: .zero, captionFrame: .zero)
    play.sendActions(for: .touchUpInside)
    XCTAssertFalse(playback.state.playing)
    view.updatePresentation(active: false, chromeVisible: true, safeArea: .zero, captionFrame: .zero)
    play.sendActions(for: .touchUpInside)
    XCTAssertFalse(playback.state.playing)
    view.retire()
    view.updatePresentation(active: true, chromeVisible: true, safeArea: .zero, captionFrame: .zero)
    play.sendActions(for: .touchUpInside)
    XCTAssertFalse(playback.state.playing)
  }
  func testScrubberTimestampsAndAdjustableActionsUseRealSourceContract() {
    let scrubber = ZoenConversationVideoScrubber(frame: CGRect(x: 0, y: 0, width: 338, height: 44))
    scrubber.playbackState = .init(position: 61.1, duration: 90.2)
    scrubber.layoutIfNeeded()
    XCTAssertEqual(ZoenConversationVideoScrubber.time(61.1), "1:01")
    XCTAssertEqual(ZoenConversationVideoScrubber.time(61.1 - 90.2), "0:30")
    XCTAssertEqual(ZoenConversationVideoScrubber.time(3661.8), "1:01:01")
    XCTAssertEqual(ZoenConversationVideoScrubber.time(.infinity), "-:--")
    XCTAssertEqual(scrubber.accessibilityValue, "1:01 / 1:30")
    XCTAssertTrue(scrubber.accessibilityTraits.contains(.adjustable))
    var deltas: [Double] = []
    scrubber.adjust = { deltas.append($0) }
    scrubber.accessibilityIncrement(); scrubber.accessibilityDecrement()
    XCTAssertEqual(deltas, [15, -15])
    XCTAssertEqual(scrubber.bounds.height, 44)
  }
  func testUIKitMissingEndTouchCannotCommitCancelledScrub() {
    let scrubber = ZoenConversationVideoScrubber(frame: CGRect(x: 0, y: 0, width: 338, height: 44))
    var endings: [Bool] = []
    scrubber.ended = { endings.append($0) }
    // UIControl.h explicitly permits this call during cancellation.
    scrubber.endTracking(nil, with: nil)
    XCTAssertEqual(endings, [false])
  }
  func testActualGalleryLongCaptionCannotCoverPlayOrScrubberInEitherSizeClass() throws {
    let file = URL.temporaryDirectory.appendingPathComponent("zoen-video-layout-\(UUID().uuidString).mp4")
    try Data("Synthetic layout-only media; playback is separately host-tested".utf8).write(to: file)
    defer { try? FileManager.default.removeItem(at: file) }
    let appearance = ZoenKeyboardPanelAppearance(background: .black, text: .white, secondary: .gray, accent: .white, separator: .darkGray, bottomInset: 0)
    for (size, safe, sizeClass) in [
      (CGSize(width: 390, height: 844), UIEdgeInsets(top: 59, left: 0, bottom: 34, right: 0), UIUserInterfaceSizeClass.compact),
      (CGSize(width: 844, height: 390), UIEdgeInsets(top: 0, left: 59, bottom: 21, right: 59), .compact),
      (CGSize(width: 844, height: 390), UIEdgeInsets(top: 0, left: 59, bottom: 21, right: 59), .regular)
    ] {
      let gallery = ZoenAttachmentGalleryContent { _, _ in }
      gallery.view.traitOverrides.horizontalSizeClass = sizeClass
      gallery.view.frame = CGRect(origin: .zero, size: size)
      try gallery.update(model: ["source": "conversation", "screen": "preview", "previewId": "a", "revision": "one", "selected": [String](),
        "items": [["id": "a", "name": "Synthetic video", "mime": "video/mp4", "uri": file.absoluteString]],
        "caption": ["value": String(repeating: "A real long caption must keep playback reachable. ", count: 100)]], appearance: appearance)
      gallery.updatePresentation(keyboardHeight: 0, safeArea: safe)
      gallery.view.layoutIfNeeded()
      let caption = try XCTUnwrap(descendants(gallery.view).first { $0.accessibilityIdentifier == "conversation-gallery-caption" } as? UITextView)
      let play = try control(gallery.view, "conversation-video-play-pause")
      let scrubber = try control(gallery.view, "conversation-video-scrubber")
      let playFrame = play.convert(play.bounds, to: gallery.view), scrubberFrame = scrubber.convert(scrubber.bounds, to: gallery.view)
      XCTAssertFalse(playFrame.intersects(scrubberFrame))
      XCTAssertEqual(scrubberFrame.height, 44)
      if size.width > size.height && sizeClass == .compact {
        XCTAssertTrue(caption.isHidden)
      } else {
        XCTAssertFalse(caption.isHidden)
        XCTAssertLessThanOrEqual(caption.frame.height, 80)
        XCTAssertTrue(caption.isScrollEnabled)
        XCTAssertFalse(caption.frame.intersects(playFrame))
        XCTAssertFalse(caption.frame.intersects(scrubberFrame))
      }
      XCTAssertEqual(descendants(gallery.view).filter { $0.accessibilityIdentifier == "conversation-gallery-close" }.count, 1)
      gallery.retire()
    }
  }
}
