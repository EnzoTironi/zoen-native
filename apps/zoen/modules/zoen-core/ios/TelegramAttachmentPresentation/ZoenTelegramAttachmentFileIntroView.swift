import Foundation
import Lottie
import UIKit

// Telegram AttachmentFileEmptyItem.swift:73–82,122–136 at
// f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838; GPLv3 selected.
// The original Files animation loops at 144pt (112pt for width320), with its
// original colors. The owning attachment child supplies layout and visibility.
final class ZoenTelegramAttachmentFileIntroView: UIView {
  private let animationView: LottieAnimationView
  private var visible = false
  private var retired = false
  private var motionObserver: NSObjectProtocol?

  override init(frame: CGRect) {
    animationView = LottieAnimationView(
      animation: LottieAnimation.named("Files", bundle: ZoenTelegramAttachmentGlyph.bundle),
      configuration: LottieConfiguration(renderingEngine: .mainThread))
    super.init(frame: frame)
    isUserInteractionEnabled = false
    animationView.isUserInteractionEnabled = false
    animationView.contentMode = .scaleAspectFit
    animationView.loopMode = .loop
    animationView.backgroundBehavior = .pauseAndRestore
    if let animation = animationView.animation { animationView.currentFrame = animation.startFrame }
    addSubview(animationView)
    motionObserver = NotificationCenter.default.addObserver(
      forName: UIAccessibility.reduceMotionStatusDidChangeNotification, object: nil, queue: .main
    ) { [weak self] _ in
      MainActor.assumeIsolated { self?.updatePlayback() }
    }
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
  deinit { if let motionObserver { NotificationCenter.default.removeObserver(motionObserver) } }
  override func layoutSubviews() { super.layoutSubviews(); animationView.frame = bounds }
  override func didMoveToWindow() { super.didMoveToWindow(); updatePlayback() }
  func setVisible(_ value: Bool) { visible = value; updatePlayback() }
  func retire() { retired = true; visible = false; updatePlayback() }
  private func updatePlayback() {
    guard !retired, visible, window != nil, !isHidden else { animationView.pause(); return }
    if UIAccessibility.isReduceMotionEnabled {
      animationView.pause()
      if let animation = animationView.animation { animationView.currentFrame = animation.startFrame }
      animationView.forceDisplayUpdate()
    } else if !animationView.isAnimationPlaying {
      animationView.play()
    }
  }
}
