// Native Expo adapter for the pinned Telegram presentation components.
// GPL-3.0 selected for the upstream-derived implementation. See NOTICE.md.
import Foundation
import UIKit
import Lottie

private final class ZoenTelegramAttachmentResources: NSObject {}

enum ZoenTelegramAttachmentGlyph: String, CaseIterable {
  case gallery, file, audio, location, contact, poll
  case camera, openCamera, send, composerSend, close, more, keyboard
  case back, titleExpand, pencil, check, selected, recipientArrow, cloud, image, crop, rotate, done, cropCorners, play

  static let bundle: Bundle = {
    let owner = Bundle(for: ZoenTelegramAttachmentResources.self)
    guard let url = owner.url(forResource: "ZoenTelegramAttachmentPresentation", withExtension: "bundle"),
      let resources = Bundle(url: url) else { return owner }
    return resources
  }()

  fileprivate var animationName: String? {
    switch self {
    case .gallery: return "TabPhoto"
    case .file: return "TabFile"
    case .audio: return "TabAudio"
    case .location: return "TabLocation"
    case .contact: return "TabContacts"
    case .poll: return "TabPoll"
    default: return nil
    }
  }

  // Original category icons use a 30pt nominal slot, with the source contact
  // animation's additional 1.78 scale and one-device-pixel vertical offset.
  fileprivate var nominalSize: CGSize {
    switch self {
    case .gallery, .file, .audio, .location, .contact, .poll: return CGSize(width: 30, height: 30)
    default: return .zero
    }
  }

  static func image(_ glyph: Self, pointSize: CGSize? = nil) -> UIImage? {
    if let animationName = glyph.animationName,
      let animation = LottieAnimation.named(animationName, bundle: bundle) {
      let view = LottieAnimationView(animation: animation,
        configuration: LottieConfiguration(renderingEngine: .mainThread))
      let size = pointSize ?? glyph.nominalSize
      view.frame = CGRect(origin: .zero, size: size)
      view.contentMode = .scaleAspectFit
      view.currentFrame = animation.startFrame
      view.layoutIfNeeded()
      view.forceDisplayUpdate()
      return UIGraphicsImageRenderer(size: size).image { renderer in
        view.layer.render(in: renderer.cgContext)
      }.withRenderingMode(.alwaysTemplate)
    }
    if let url = bundle.url(forResource: glyph.rawValue, withExtension: "pdf"),
      let document = CGPDFDocument(url as CFURL), let page = document.page(at: 1) {
      let size = pointSize ?? page.getBoxRect(.mediaBox).size
      guard size.width > 0, size.height > 0 else { return nil }
      return UIGraphicsImageRenderer(size: size).image { renderer in
        let context = renderer.cgContext
        context.translateBy(x: 0, y: size.height)
        context.scaleBy(x: 1, y: -1)
        context.concatenate(page.getDrawingTransform(.mediaBox,
          rect: CGRect(origin: .zero, size: size), rotate: 0, preserveAspectRatio: true))
        context.drawPDFPage(page)
      }.withRenderingMode(.alwaysTemplate)
    }
    guard let image = UIImage(named: glyph.rawValue, in: bundle, compatibleWith: nil) else { return nil }
    guard let size = pointSize else { return image.withRenderingMode(.alwaysTemplate) }
    return UIGraphicsImageRenderer(size: size).image { _ in
      image.draw(in: CGRect(origin: .zero, size: size))
    }.withRenderingMode(.alwaysTemplate)
  }
}

// Telegram AnimatedStickerComponent uses still(.start) until isAnimating, then
// once + playIfNeeded. The original unmodified TGS payload is decompressed into
// the JSON consumed by the official Lottie renderer; no substitute glyph/path.
final class ZoenTelegramAttachmentAnimationView: UIView {
  private var animationView: LottieAnimationView?
  private var glyph: ZoenTelegramAttachmentGlyph?
  private var wasAnimating = false
  private let tintSurface = UIView()

  override init(frame: CGRect) {
    super.init(frame: frame)
    isUserInteractionEnabled = false
    addSubview(tintSurface)
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

  func update(glyph: ZoenTelegramAttachmentGlyph, tintColor: UIColor,
              isAnimating: Bool, reducedMotion: Bool = UIAccessibility.isReduceMotionEnabled) {
    let changed = self.glyph != glyph
    if changed {
      animationView?.stop()
      animationView?.removeFromSuperview()
      animationView = nil
      self.glyph = glyph
      if let name = glyph.animationName,
        let animation = LottieAnimation.named(name, bundle: ZoenTelegramAttachmentGlyph.bundle) {
        let view = LottieAnimationView(animation: animation,
          configuration: LottieConfiguration(renderingEngine: .mainThread))
        view.isUserInteractionEnabled = false
        view.contentMode = .scaleAspectFit
        view.backgroundBehavior = .pauseAndRestore
        view.currentFrame = animation.startFrame
        animationView = view
        tintSurface.mask = nil
        addSubview(view)
      } else {
        let imageView = UIImageView(image: ZoenTelegramAttachmentGlyph.image(glyph))
        imageView.contentMode = .scaleAspectFit
        tintSurface.mask = imageView
      }
    }
    tintSurface.backgroundColor = tintColor
    if let view = animationView {
      view.setValueProvider(ColorValueProvider(tintColor.lottieColorValue),
        keypath: AnimationKeypath(keypath: "**.Color"))
      tintSurface.isHidden = true
    } else {
      tintSurface.isHidden = false
    }
    setNeedsLayout()
    layoutIfNeeded()
    if let view = animationView, let animation = view.animation {
      if reducedMotion {
        view.pause()
        view.currentFrame = animation.startFrame
        view.forceDisplayUpdate()
      } else if changed || !wasAnimating {
        view.play(fromFrame: animation.startFrame, toFrame: animation.endFrame,
          loopMode: .playOnce)
      }
    }
    wasAnimating = isAnimating
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    tintSurface.frame = bounds
    let scale: CGFloat = glyph == .contact ? 1.78 : 1.0
    let offset: CGFloat = glyph == .contact ? 1.0 / max(1.0, traitCollection.displayScale) : 0
    let iconFrame = CGRect(x: bounds.midX - bounds.width * scale * 0.5,
      y: bounds.midY - bounds.height * scale * 0.5 + offset,
      width: bounds.width * scale, height: bounds.height * scale)
    animationView?.frame = iconFrame
    tintSurface.mask?.frame = iconFrame
  }
}

// These wrappers own only presentation. They use the same original native
// GlassBackgroundView/Container and LiquidLens classes as the category panel.
final class ZoenTelegramAttachmentGlassView: UIView {
  let glassView = GlassBackgroundView(frame: .zero)
  var contentView: UIView { glassView.contentView }
  private var isDark = false
  private var radius: CGFloat = 0
  private var interactive = true
  private var tintKind: GlassBackgroundView.TintColor.Kind = .panel

  override init(frame: CGRect) {
    ZoenTelegramInstallPresentationRuntime()
    super.init(frame: frame)
    addSubview(glassView)
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
  func update(isDark: Bool, cornerRadius: CGFloat, isInteractive: Bool = true,
              tintKind: GlassBackgroundView.TintColor.Kind = .panel) {
    self.isDark = isDark; radius = cornerRadius; interactive = isInteractive
    self.tintKind = tintKind
    setNeedsLayout()
    updateGlass()
  }
  private func updateGlass() {
    glassView.frame = bounds
    glassView.update(size: bounds.size, cornerRadius: radius, isDark: isDark,
      tintColor: .init(kind: tintKind), isInteractive: interactive, transition: .immediate)
  }
  override func layoutSubviews() { super.layoutSubviews(); updateGlass() }
}

final class ZoenTelegramAttachmentGlassContainerView: UIView {
  private let glassContainer: GlassBackgroundContainerView
  var contentView: UIView { glassContainer.contentView }
  private var isDark = false
  init(spacing: CGFloat = 7.0) {
    ZoenTelegramInstallPresentationRuntime()
    glassContainer = GlassBackgroundContainerView(spacing: spacing)
    super.init(frame: .zero)
    addSubview(glassContainer)
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
  func update(isDark: Bool) { self.isDark = isDark; setNeedsLayout() }
  override func layoutSubviews() {
    super.layoutSubviews()
    glassContainer.frame = bounds
    glassContainer.update(size: bounds.size, isDark: isDark, transition: .immediate)
  }
}
