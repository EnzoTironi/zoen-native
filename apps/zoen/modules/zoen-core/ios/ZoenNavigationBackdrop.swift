import ExpoModulesCore
import UIKit

/// A navigation scrim samples the real scrolling content instead of covering it
/// with wallpaper. Its alpha mask fades the native material into the timeline.
@MainActor final class ZoenNavigationBackdrop: ExpoView {
  private let backdrop = UIView()
  private let effect = UIVisualEffectView()
  private let fade = CAGradientLayer()
  private var appearance: String?
  var isDark = false
  var solidHeight: Double = 0

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    isUserInteractionEnabled = false
    isAccessibilityElement = false
    accessibilityElementsHidden = true
    backdrop.isUserInteractionEnabled = false
    effect.isUserInteractionEnabled = false
    backdrop.addSubview(effect)
    addSubview(backdrop)
    fade.colors = [UIColor.black.cgColor, UIColor.black.cgColor, UIColor.clear.cgColor]
    fade.startPoint = CGPoint(x: 0.5, y: 0)
    fade.endPoint = CGPoint(x: 0.5, y: 1)
    backdrop.layer.mask = fade
    NotificationCenter.default.addObserver(self, selector: #selector(transparencyChanged),
      name: UIAccessibility.reduceTransparencyStatusDidChangeNotification, object: nil)
    apply()
  }

  deinit { NotificationCenter.default.removeObserver(self) }

  @objc private func transparencyChanged() { apply() }

  func apply() {
    let reduced = UIAccessibility.isReduceTransparencyEnabled
    let next = "\(isDark):\(reduced)"
    if appearance != next {
      appearance = next
      overrideUserInterfaceStyle = isDark ? .dark : .light
      // UIKit owns backdrop sampling and contrast. No private layer filters,
      // screenshot copies, opacity animation or shadow around the whole bar.
      effect.effect = reduced ? nil : UIBlurEffect(style: .systemUltraThinMaterial)
      backdrop.backgroundColor = reduced ? .systemBackground : .clear
    }
    setNeedsLayout()
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    let solid = solidHeight.isFinite ? max(0, CGFloat(solidHeight)) : 0
    let stop = bounds.height > 0 ? min(1, solid / bounds.height) : 0
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    backdrop.frame = bounds
    effect.frame = backdrop.bounds
    fade.frame = backdrop.bounds
    fade.locations = [0, NSNumber(value: Double(stop)), 1]
    CATransaction.commit()
  }
}
