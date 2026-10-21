import UIKit

/// ContextControllerTakeViewInfo/ItemContentNode's UIKit-owned content path.
/// Only the row's dedicated owned child moves. UITableViewCell.contentView and
/// all UIKit/React-managed views stay in place. Return precedes reuse/reload.
@MainActor final class ZoenAttachmentExtractedContent {
  let anchor: UIView
  let content: UIView
  private let updateExtracted: (Bool, ComponentTransition) -> Void
  private var originalIndex: Int?
  private var originalInteraction = true
  private weak var staging: UIView?
  private var originalSize = CGSize.zero
  private var taken = false

  init(anchor: UIView, content: UIView,
       updateExtracted: @escaping (Bool, ComponentTransition) -> Void) {
    self.anchor = anchor; self.content = content; self.updateExtracted = updateExtracted
  }
  func take(into staging: UIView) -> Bool {
    guard !taken, content.superview === anchor, anchor.window != nil else { return false }
    taken = true; self.staging = staging
    originalIndex = anchor.subviews.firstIndex(of: content)
    originalSize = content.bounds.size
    originalInteraction = content.isUserInteractionEnabled
    content.removeFromSuperview(); staging.addSubview(content)
    content.frame = CGRect(origin: .zero, size: originalSize)
    content.isUserInteractionEnabled = false
    updateExtracted(true, UIAccessibility.isReduceMotionEnabled ? .immediate : .spring(duration: 0.5))
    return true
  }
  func layout(in staging: UIView, frame: CGRect) {
    let size = originalSize == .zero ? content.bounds.size : originalSize
    guard size.width > 0, size.height > 0 else { return }
    // Mirror the containing sheet's visual scale when crossing into an
    // unscaled overlay, as ItemContentNode:783–795 does for its offset node.
    staging.bounds = CGRect(origin: .zero, size: size)
    staging.center = CGPoint(x: frame.midX, y: frame.midY)
    staging.transform = CGAffineTransform(scaleX: frame.width / size.width, y: frame.height / size.height)
  }
  func beginReturn(animated: Bool) {
    updateExtracted(false, animated && !UIAccessibility.isReduceMotionEnabled ? .easeInOut(duration: 0.2) : .immediate)
  }
  func restore() {
    guard taken else { return }
    taken = false
    updateExtracted(false, .immediate)
    content.layer.removeAllAnimations()
    if content.superview === staging {
      content.removeFromSuperview()
      anchor.insertSubview(content, at: min(originalIndex ?? anchor.subviews.count, anchor.subviews.count))
      // The dedicated row fills its source. Rotation can resize the source
      // while its body is away; restore into the live bounds before reuse.
      content.frame = anchor.bounds
      anchor.superview?.setNeedsLayout()
    }
    content.isUserInteractionEnabled = originalInteraction
    staging = nil; originalIndex = nil
  }
}

/// NavigationBackgroundView.swift:62–113,120–139 (NavigationBackgroundNode).
/// The actual .light blur/filter selection is retained, including Reduce
/// Transparency. No .systemMaterial replacement or Photos reference blur.
@MainActor final class ZoenAttachmentContextBackdrop: UIView {
  private let fill = UIView()
  private var effect: UIVisualEffectView?
  init(isDark: Bool) {
    super.init(frame: .zero)
    isUserInteractionEnabled = false
    let dim = isDark ? UIColor(white: 0, alpha: 0.6)
      : UIColor(red: 0, green: 10 / 255, blue: 38 / 255, alpha: 0.2)
    if UIAccessibility.isReduceTransparencyEnabled {
      fill.backgroundColor = dim.withAlphaComponent(1)
    } else {
      let effect = UIVisualEffectView(effect: UIBlurEffect(style: .light))
      for child in effect.subviews where child.description.contains("VisualEffectSubview") { child.isHidden = true }
      if let layer = effect.layer.sublayers?.first, let filters = layer.filters {
        layer.backgroundColor = nil; layer.isOpaque = false
        layer.filters = filters.filter { filter in
          guard let filter = filter as? NSObject else { return true }
          return ["gaussianBlur", "colorSaturate"].contains(String(describing: filter))
        }
      }
      self.effect = effect; addSubview(effect)
      fill.backgroundColor = dim
    }
    addSubview(fill)
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
  override func layoutSubviews() {
    super.layoutSubviews(); fill.frame = bounds; effect?.frame = bounds
    effect?.layer.sublayers?.forEach { $0.frame = bounds }
  }
}
