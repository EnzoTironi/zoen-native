// SPDX-License-Identifier: GPL-3.0-or-later
// UIKit adapter of Telegram-iOS f1dd7a2 VideoPlaybackControlsComponent,
// ChatVideoGalleryItemScrubberView and MediaPlayerScrubbingNode. See SOURCES.json.
import UIKit
import Lottie

@MainActor final class ZoenConversationVideoControls: UIView {
  static let centerDiameter: CGFloat = 92
  /// The caption cannot cover either the centered transport or its44pt footer.
  /// This is only a viewport clamp over the pinned footer100→80pt text rule.
  static func minimumCaptionY(height: CGFloat) -> CGFloat { height / 2 + centerDiameter / 2 + 44 + 10 }
  let scrubber = ZoenConversationVideoScrubber()
  private let centerButton = ZoenConversationPlaybackButton(direction: nil)
  private let backward = ZoenConversationPlaybackButton(direction: false)
  private let forward = ZoenConversationPlaybackButton(direction: true)
  private let animation = LottieAnimationView(animation: LottieAnimation.named("ConversationPlayPause",
    bundle: ZoenTelegramAttachmentGlyph.bundle), configuration: LottieConfiguration(renderingEngine: .mainThread))
  private weak var playback: ZoenConversationVideoPlayback?
  private var lastPlaying: Bool?
  private var chromeVisible = true
  private var active = false
  private var retired = false
  private var safeArea = UIEdgeInsets.zero
  private var captionFrame = CGRect.zero
  var interacting: ((Bool) -> Void)?

  init(playback: ZoenConversationVideoPlayback) {
    self.playback = playback
    super.init(frame: .zero)
    accessibilityIdentifier = "conversation-video-controls"
    for child in [backward, centerButton, forward, scrubber] { addSubview(child) }
    centerButton.accessibilityIdentifier = "conversation-video-play-pause"
    backward.accessibilityIdentifier = "conversation-video-seek-backward"
    forward.accessibilityIdentifier = "conversation-video-seek-forward"
    backward.accessibilityLabel = NSLocalizedString("Back 15 seconds", comment: "Conversation video playback")
    forward.accessibilityLabel = NSLocalizedString("Forward 15 seconds", comment: "Conversation video playback")
    centerButton.addAction(UIAction { [weak self] _ in self?.action { $0.toggle() } }, for: .touchUpInside)
    backward.addAction(UIAction { [weak self] _ in self?.action { $0.seek(by: -15) } }, for: .touchUpInside)
    forward.addAction(UIAction { [weak self] _ in self?.action { $0.seek(by: 15) } }, for: .touchUpInside)
    animation.isUserInteractionEnabled = false; animation.contentMode = .scaleAspectFit
    animation.backgroundBehavior = .pause
    animation.animationSpeed = 1
    centerButton.addSubview(animation)
    scrubber.began = { [weak self] in self?.action { $0.beginScrubbing() } }
    scrubber.changed = { [weak self] fraction in self?.action { $0.updateScrubbing(fraction: fraction) } }
    scrubber.ended = { [weak self] apply in self?.action { $0.endScrubbing(apply: apply) } }
    scrubber.adjust = { [weak self] delta in self?.action { $0.seek(by: delta) } }
    playback.changed = { [weak self] state in self?.update(state: state) }
    update(state: playback.state)
  }
  required init?(coder: NSCoder) { nil }

  func updatePresentation(active: Bool, chromeVisible: Bool, safeArea: UIEdgeInsets, captionFrame: CGRect) {
    guard !retired else { return }
    if !chromeVisible { scrubber.cancelTracking(with: nil); playback?.endScrubbing(apply: false) }
    self.active = active; self.chromeVisible = chromeVisible
    self.safeArea = safeArea; self.captionFrame = captionFrame
    applyVisibility(); setNeedsLayout()
  }
  private func action(_ body: (ZoenConversationVideoPlayback) -> Void) {
    guard active, chromeVisible, !retired, let playback else { return }; body(playback)
  }
  private func update(state: ZoenConversationVideoPlayback.State) {
    let wasScrubbing = scrubber.playbackState.scrubbing
    scrubber.playbackState = state
    backward.isEnabled = active && state.seekable
    forward.isEnabled = active && state.seekable
    backward.isHidden = !state.seekable; forward.isHidden = !state.seekable
    centerButton.isEnabled = active && !state.failed
    centerButton.isHidden = state.failed
    centerButton.accessibilityLabel = NSLocalizedString(state.playing ? "Pause video" : "Play video", comment: "Conversation video playback")
    if lastPlaying != state.playing {
      // Original anim_playpause: play→pause0...41, pause→play41...83,0.35s.
      let animated = lastPlaying != nil && !UIAccessibility.isReduceMotionEnabled && chromeVisible && active
      animation.pause()
      if animated {
        animation.animationSpeed = (state.playing ? 41 : 42) / (0.35 * (animation.animation?.framerate ?? 60))
        animation.play(fromFrame: state.playing ? 0 : 41, toFrame: state.playing ? 41 : 83, loopMode: .playOnce)
      } else { animation.currentFrame = state.playing ? 41 : 0; animation.forceDisplayUpdate() }
      lastPlaying = state.playing
      setNeedsLayout()
    }
    if wasScrubbing != state.scrubbing { interacting?(state.scrubbing) }
    applyVisibility()
  }
  private func applyVisibility() {
    isUserInteractionEnabled = active && chromeVisible
    accessibilityElementsHidden = !active || !chromeVisible
    let value: CGFloat = chromeVisible && active ? 1 : 0
    if alpha != value {
      UIView.animate(withDuration: UIAccessibility.isReduceMotionEnabled ? 0 : 0.2,
        delay: 0, options: [.beginFromCurrentState, .curveLinear]) { self.alpha = value }
    }
    if !active { animation.pause() }
    if let state = playback?.state {
      centerButton.isEnabled = active && !state.failed
      backward.isEnabled = active && state.seekable; forward.isEnabled = active && state.seekable
    }
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    // UniversalVideoGalleryItem1281–1318. No additional AVKit close or toolbar.
    let side: CGFloat = 64, middle = Self.centerDiameter, spacing: CGFloat = 30
    let width = side * 2 + middle + spacing * 2, x = floor((bounds.width - width) / 2)
    backward.frame = CGRect(x: x, y: floor((bounds.height - side) / 2), width: side, height: side)
    centerButton.frame = CGRect(x: x + side + spacing, y: floor((bounds.height - middle) / 2), width: middle, height: middle)
    forward.frame = CGRect(x: x + width - side, y: backward.frame.minY, width: side, height: side)
    let icon: CGFloat = middle * 0.9
    animation.frame = CGRect(x: (middle - icon) / 2 + (lastPlaying == true ? 0 : 5), y: (middle - icon) / 2, width: icon, height: icon)
    // Footer1178–1330:44pt row,8pt bottom,26pt sides on phones. The
    // existing read-only caption occupies the otherwise shared footer content.
    let bottom = captionFrame.isEmpty ? bounds.height - safeArea.bottom - 8 : captionFrame.minY - 10
    let sideInset: CGFloat = safeArea.bottom <= 32 ? 26 : 8
    scrubber.frame = CGRect(x: safeArea.left + sideInset, y: max(safeArea.top + 44, bottom - 44),
      width: max(0, bounds.width - safeArea.left - safeArea.right - sideInset * 2), height: 44)
  }
  override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
    guard active, chromeVisible, !isHidden, alpha > 0.01 else { return nil }
    let hit = super.hitTest(point, with: event)
    return hit === self ? nil : hit
  }
  func retire() {
    retired = true; active = false; animation.stop(); isUserInteractionEnabled = false
    scrubber.cancelTracking(with: nil); accessibilityElementsHidden = true
    interacting = nil; playback = nil
  }
}

/// The original viewer glass/glyph metrics, adapted from ComponentFlow to an
/// accessible UIKit control. Glass is visual; its sibling owns the44pt+ target.
@MainActor private final class ZoenConversationPlaybackButton: UIControl {
  private let glass = ZoenTelegramAttachmentGlassView()
  private let icon = UIImageView()
  private let text = UILabel()
  private let direction: Bool?
  init(direction: Bool?) {
    self.direction = direction
    super.init(frame: .zero)
    addSubview(glass); glass.isUserInteractionEnabled = false
    addSubview(icon); addSubview(text)
    icon.isUserInteractionEnabled = false; text.isUserInteractionEnabled = false
    isAccessibilityElement = true; accessibilityTraits = .button
    if let direction {
      icon.image = Self.image(direction ? "conversationForward" : "conversationBackward")
      icon.tintColor = .white
      text.text = "15"; text.textColor = .white; text.textAlignment = .center
      text.font = .systemFont(ofSize: 16, weight: .semibold).rounded()
    }
  }
  required init?(coder: NSCoder) { nil }
  override var isHighlighted: Bool {
    didSet {
      guard let direction else { return }
      // PlaybackIconView: .18linear press, .2linear release, π/4+.226.
      UIView.animate(withDuration: UIAccessibility.isReduceMotionEnabled ? 0 : (isHighlighted ? 0.18 : 0.2),
        delay: 0, options: [.beginFromCurrentState, .curveLinear]) {
        self.icon.transform = self.isHighlighted ? CGAffineTransform(rotationAngle: (direction ? 1 : -1) * (.pi / 4 + 0.226)) : .identity
      }
    }
  }
  override func layoutSubviews() {
    super.layoutSubviews(); glass.frame = bounds
    glass.update(isDark: true, cornerRadius: bounds.height / 2, isInteractive: false,
      tintKind: .custom(style: .clear, color: UIColor(white: 0, alpha: 0.2)))
    if let image = icon.image {
      let size = CGSize(width: floor(image.size.width * 1.4), height: floor(image.size.height * 1.4))
      icon.bounds = CGRect(origin: .zero, size: size); icon.center = CGPoint(x: bounds.midX, y: bounds.midY)
    }
    text.sizeToFit(); text.center = CGPoint(x: bounds.midX, y: bounds.midY + 1 / max(1, traitCollection.displayScale))
  }
  private static func image(_ name: String) -> UIImage? {
    guard let url = ZoenTelegramAttachmentGlyph.bundle.url(forResource: name, withExtension: "pdf"),
      let document = CGPDFDocument(url as CFURL), let page = document.page(at: 1) else { return nil }
    let size = page.getBoxRect(.mediaBox).size
    return UIGraphicsImageRenderer(size: size).image { renderer in
      renderer.cgContext.translateBy(x: 0, y: size.height); renderer.cgContext.scaleBy(x: 1, y: -1)
      renderer.cgContext.drawPDFPage(page)
    }.withRenderingMode(.alwaysTemplate)
  }
}

private extension UIFont {
  func rounded() -> UIFont { UIFont(descriptor: fontDescriptor.withDesign(.rounded) ?? fontDescriptor, size: pointSize) }
}

/// Source .standard(lineHeight:8,lineCap:.round,handle:.none), timestamps13pt,
///58pt side reservations and the original relative-drag/cancel contract.
@MainActor final class ZoenConversationVideoScrubber: UIControl {
  private let glass = ZoenTelegramAttachmentGlassView()
  private let background = UIView(), foreground = UIView()
  private let elapsed = UILabel(), remaining = UILabel()
  private var origin = CGPoint.zero
  private var track = CGRect.zero
  var began: (() -> Void)?, changed: ((Double) -> Void)?, ended: ((Bool) -> Void)?, adjust: ((Double) -> Void)?
  var playbackState = ZoenConversationVideoPlayback.State() { didSet { update() } }
  override init(frame: CGRect) {
    super.init(frame: frame)
    for child in [glass, background, foreground, elapsed, remaining] { addSubview(child); child.isUserInteractionEnabled = false }
    glass.isUserInteractionEnabled = false
    background.backgroundColor = UIColor(white: 1, alpha: 0.42); foreground.backgroundColor = .white
    background.layer.cornerRadius = 4; foreground.layer.cornerRadius = 4
    for label in [elapsed, remaining] { label.textColor = .white; label.font = .monospacedDigitSystemFont(ofSize: 13, weight: .medium) }
    remaining.textAlignment = .right
    accessibilityIdentifier = "conversation-video-scrubber"
    accessibilityLabel = NSLocalizedString("Video position", comment: "Read-only video scrubber")
    isAccessibilityElement = true; accessibilityTraits = .adjustable
    update()
  }
  required init?(coder: NSCoder) { nil }
  private func update() {
    elapsed.text = Self.time(playbackState.position)
    remaining.text = playbackState.seekable ? Self.time(playbackState.position - playbackState.duration) : "-:--"
    accessibilityValue = "\(elapsed.text ?? "") / \(Self.time(playbackState.duration))"
    isEnabled = playbackState.seekable
    setNeedsLayout()
  }
  static func time(_ seconds: Double) -> String {
    guard seconds.isFinite, abs(seconds) < Double(Int32.max) else { return "-:--" }
    let value = abs(Int(floor(seconds)))
    let text = value >= 3600 ? String(format: "%d:%02d:%02d", value / 3600, value % 3600 / 60, value % 60) : String(format: "%d:%02d", value / 60, value % 60)
    return text
  }
  override func layoutSubviews() {
    super.layoutSubviews(); glass.frame = bounds; glass.update(isDark: true, cornerRadius: 22, isInteractive: false)
    var left: CGFloat = 58, right: CGFloat = 58
    if let value = elapsed.text, value.count > 4 { left = min(100, 16 + elapsed.sizeThatFits(CGSize(width: 200, height: 20)).width + 8) }
    if let value = remaining.text, value.count > 4 { right = min(100, 16 + remaining.sizeThatFits(CGSize(width: 200, height: 20)).width + 8) }
    elapsed.frame = CGRect(x: 16, y: 14, width: max(0, left - 24), height: 20)
    remaining.frame = CGRect(x: bounds.width - right + 8, y: 14, width: max(0, right - 24), height: 20)
    // Original scrubber wrapper y15,height14;8pt line is centered inside it.
    track = CGRect(x: left, y: 18, width: max(1, bounds.width - left - right), height: 8)
    background.frame = track
    let fraction = playbackState.seekable ? min(1, max(0, playbackState.position / playbackState.duration)) : 0
    let scale = max(1, traitCollection.displayScale)
    foreground.frame = CGRect(x: track.minX, y: track.minY, width: floor(track.width * fraction * scale) / scale, height: 8)
  }
  override func beginTracking(_ touch: UITouch, with event: UIEvent?) -> Bool {
    guard isEnabled else { return false }
    origin = touch.location(in: self); began?(); return true
  }
  override func continueTracking(_ touch: UITouch, with event: UIEvent?) -> Bool {
    changed?(Double((touch.location(in: self).x - origin.x) / max(1, track.width))); return true
  }
  override func endTracking(_ touch: UITouch?, with event: UIEvent?) {
    if let touch { changed?(Double((touch.location(in: self).x - origin.x) / max(1, track.width))) }
    // UIKit may call endTracking(nil) from cancelTracking. It cannot commit.
    ended?(touch != nil)
  }
  override func cancelTracking(with event: UIEvent?) { super.cancelTracking(with: event); ended?(false) }
  override func accessibilityIncrement() { adjust?(15) }
  override func accessibilityDecrement() { adjust?(-15) }
}
