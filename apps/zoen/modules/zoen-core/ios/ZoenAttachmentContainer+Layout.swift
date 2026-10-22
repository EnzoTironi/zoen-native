import UIKit

@MainActor extension ZoenAttachmentContainer {
  func applyLayout(updateDim: Bool = true) {
    dim.frame = bounds
    let regular = traitCollection.horizontalSizeClass == .regular
    let layout = ZoenAttachmentContainerMetrics.layout(size: bounds.size, baseTop: baseTop,
      offset: sheetY - baseTop, inset: inset, landscape: landscape, regular: regular,
      fullScreen: fullScreen, scale: traitCollection.displayScale)
    wrapping.frame = layout.wrapping
    sheet.bounds = CGRect(origin: .zero, size: layout.clip.size)
    sheet.center = CGPoint(x: layout.clip.midX, y: layout.clip.midY)
    sheet.layer.transform = CATransform3DMakeScale(layout.clipScale, layout.clipScale, 1)
    sheet.layer.cornerRadius = landscape || fullScreen || regular ? 0 : 38
    sheet.layer.maskedCorners = [.layerMinXMinYCorner, .layerMaxXMinYCorner, .layerMinXMaxYCorner, .layerMaxXMaxYCorner]
    bottomClip.frame = layout.bottomClip
    let profileKey = CornerProfileKey(size: window?.screen.bounds.size ?? bounds.size,
      scale: traitCollection.displayScale, statusBarHeight: safeAreaInsets.top, homeIndicator: safeAreaInsets.bottom > 0)
    if cornerProfile?.key != profileKey {
      cornerProfile = (profileKey, ZoenAttachmentScreenCornerRadius(size: profileKey.size, scale: profileKey.scale,
        statusBarHeight: profileKey.statusBarHeight, homeIndicator: profileKey.homeIndicator))
    }
    bottomClip.layer.cornerRadius = fullScreen ? 0 : max(24, cornerProfile?.radius ?? 0) - 2
    navigation.frame = layout.navigation
    content.view.frame = navigation.bounds
    let progress = inset == 0 ? 1 : 1 - min(1, max(0, (sheetY - baseTop) / inset))
    content.updatePresentation(expansion: progress, keyboardHeight: keyboardHeight, safeArea: safeAreaInsets,
      additionalBottomInset: layout.additionalBottomInset)
    handle.frame = CGRect(x: floor((sheet.bounds.width - 36) / 2), y: 5, width: 36, height: 5)
    handle.isHidden = fullScreen || traitCollection.horizontalSizeClass == .regular
    dim.backgroundColor = fullScreen ? .clear : UIColor(white: 0, alpha: 0.25)
    if updateDim { dim.alpha = 1 }
  }

  func stopAnimation() {
    guard animator != nil || !layerMotionViews.isEmpty else { return }
    let states = motionViews().map(VisualState.init)
    if let frame = wrapping.layer.presentation()?.frame { sheetY = baseTop + frame.minY }
    if animator?.state == .active { animator?.stopAnimation(true) }
    self.animator = nil
    removeLayerMotion()
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    applyLayout(updateDim: false)
    // Retarget from the displayed positions/bounds, including the floating bar,
    // rather than jumping children to a previous animation's model end values.
    for state in states { state.restore() }
    CATransaction.commit()
  }

  func animate(to y: CGFloat, duration: Double, spring: Bool, velocity: CGFloat) {
    if spring {
      stopAnimation()
      let distance = y - sheetY
      let initialVelocity = distance == 0 ? 0 : abs(velocity / distance)
      animateLayers(to: y, duration: duration, curve: .settlement(initialVelocity: initialVelocity))
      return
    }
    stopAnimation()
    let duration = UIAccessibility.isReduceMotionEnabled ? 0 : duration
    let animator = UIViewPropertyAnimator(duration: duration, curve: .easeInOut)
    animator.addAnimations { [weak self] in self?.sheetY = y; self?.applyLayout() }
    start(animator)
  }

  func start(_ animator: UIViewPropertyAnimator) {
    self.animator = animator
    animator.addCompletion { [weak self, weak animator] _ in
      guard let self, let animator, self.animator === animator else { return }
      self.animator = nil
    }
    animator.startAnimation()
  }

  private func motionViews() -> [UIView] {
    var seen = Set<ObjectIdentifier>()
    return ([wrapping, sheet, bottomClip, navigation, dim, handle, content.view] + content.attachmentAnimatedViews)
      .filter { seen.insert(ObjectIdentifier($0)).inserted }
  }

  func removeLayerMotion() {
    for view in layerMotionViews {
      for key in ["position", "bounds", "opacity", "transform"] {
        view.layer.removeAnimation(forKey: Self.animationPrefix + key)
      }
    }
    layerMotionViews.removeAll()
  }

  func animateLayers(to y: CGFloat, duration: Double, curve: LayerCurve) {
    stopAnimation()
    let states = motionViews().map(VisualState.init)
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    sheetY = y
    applyLayout(updateDim: false)
    dim.alpha = 1
    sheet.layoutIfNeeded()
    content.view.layoutIfNeeded()
    CATransaction.commit()
    guard duration > 0, !UIAccessibility.isReduceMotionEnabled else { return }
    layerMotionViews = states.map(\.view)
    for state in states {
      let layer = state.view.layer
      if state.position != layer.position {
        addLayerMotion(layer: layer, from: NSValue(cgPoint: state.position), to: NSValue(cgPoint: layer.position), key: "position", duration: duration, curve: curve)
      }
      if state.bounds != layer.bounds {
        addLayerMotion(layer: layer, from: NSValue(cgRect: state.bounds), to: NSValue(cgRect: layer.bounds), key: "bounds", duration: duration, curve: curve)
      }
      if !CATransform3DEqualToTransform(state.transform, layer.transform) {
        addLayerMotion(layer: layer, from: NSValue(caTransform3D: state.transform), to: NSValue(caTransform3D: layer.transform), key: "transform", duration: duration, curve: curve)
      }
      if state.opacity != layer.opacity {
        if state.view === dim {
          let fade = CABasicAnimation(keyPath: "opacity")
          fade.fromValue = state.opacity; fade.toValue = layer.opacity
          fade.duration = 0.3; fade.timingFunction = CAMediaTimingFunction(name: .linear)
          layer.add(fade, forKey: Self.animationPrefix + "opacity")
        } else {
          addLayerMotion(layer: layer, from: state.opacity, to: layer.opacity, key: "opacity", duration: duration, curve: curve)
        }
      }
    }
  }

  private func addLayerMotion(layer: CALayer, from: Any, to: Any, key: String,
                              duration: Double, curve: LayerCurve) {
    let animation: CABasicAnimation
    switch curve {
    case .entrance:
      animation = CABasicAnimation(keyPath: key)
      animation.duration = duration
      animation.timingFunction = CAMediaTimingFunction(controlPoints: 0.380, 0.700, 0.125, 1.000)
    case .captionExpansion:
      let spring = CASpringAnimation(keyPath: key)
      spring.mass = 1; spring.stiffness = 555.027; spring.damping = 47.118
      spring.duration = duration
      spring.allowsOverdamping = false
      spring.timingFunction = CAMediaTimingFunction(name: .linear)
      animation = spring
    case let .settlement(initialVelocity):
      let spring = CASpringAnimation(keyPath: key)
      spring.mass = 5; spring.stiffness = 900; spring.damping = 124
      // Upstream serializes the parameters through Float in CAAnimationUtils.
      spring.initialVelocity = CGFloat(Float(initialVelocity))
      spring.duration = spring.settlingDuration
      spring.speed = Float(spring.duration / duration)
      spring.timingFunction = CAMediaTimingFunction(name: .linear)
      animation = spring
    }
    animation.fromValue = from; animation.toValue = to
    animation.isRemovedOnCompletion = true; animation.fillMode = .forwards
    let fps = Float(window?.screen.maximumFramesPerSecond ?? 60)
    if key != "opacity", fps > 61 {
      animation.preferredFrameRateRange = CAFrameRateRange(minimum: 30, maximum: fps, preferred: fps)
    }
    layer.add(animation, forKey: Self.animationPrefix + key)
  }

}


// DeviceMetrics.swift:58–70, 118–213, 231–315. These are upstream profile
// recognition measurements, never a replacement for the window's layout size.
private func ZoenAttachmentScreenCornerRadius(size: CGSize, scale: CGFloat, statusBarHeight: CGFloat, homeIndicator: Bool) -> CGFloat {
  struct Profile {
    let size: CGSize
    let radius: CGFloat
    let models: [String]
    var deviceScale: CGFloat = 3
  }
  let pixel = 1 / max(1, scale)
  let profiles: [Profile] = [
    Profile(size: CGSize(width: 320, height: 480), radius: 0, models: []),
    Profile(size: CGSize(width: 320, height: 568), radius: 0, models: []),
    Profile(size: CGSize(width: 375, height: 667), radius: 0, models: []),
    Profile(size: CGSize(width: 414, height: 736), radius: 0, models: []),
    Profile(size: CGSize(width: 375, height: 812), radius: 39, models: ["iPhone10,3", "iPhone10,6", "iPhone11,2", "iPhone12,3"]),
    Profile(size: CGSize(width: 414, height: 896), radius: 39, models: ["iPhone11,4", "iPhone11,6", "iPhone12,5"]),
    Profile(size: CGSize(width: 414, height: 896), radius: 41.5, models: ["iPhone11,8", "iPhone12,1"], deviceScale: 2),
    Profile(size: CGSize(width: 375, height: 812), radius: 44, models: ["iPhone13,1"]),
    Profile(size: CGSize(width: 390, height: 844), radius: 47 + pixel, models: ["iPhone13,2", "iPhone13,3"]),
    Profile(size: CGSize(width: 428, height: 926), radius: 53 + pixel, models: ["iPhone13,4"]),
    Profile(size: CGSize(width: 375, height: 812), radius: 0, models: ["iPhone14,4"]),
    Profile(size: CGSize(width: 390, height: 844), radius: 47 + pixel, models: ["iPhone14,5", "iPhone14,7", "iPhone17,5", "iPhone18,5", "iPhone14,2"]),
    Profile(size: CGSize(width: 428, height: 926), radius: 53 + pixel, models: ["iPhone14,3", "iPhone14,8"]),
    Profile(size: CGSize(width: 393, height: 852), radius: 55, models: ["iPhone15,2", "iPhone15,4", "iPhone16,1", "iPhone17,3"]),
    Profile(size: CGSize(width: 320, height: 693), radius: 47 + pixel, models: ["iPhone15,2", "iPhone15,4", "iPhone16,1", "iPhone17,3"]),
    Profile(size: CGSize(width: 430, height: 932), radius: 55, models: ["iPhone15,3", "iPhone15,5", "iPhone16,2", "iPhone17,4"]),
    Profile(size: CGSize(width: 375, height: 812), radius: 53 + pixel, models: ["iPhone15,3", "iPhone15,5", "iPhone16,2", "iPhone17,4"]),
    Profile(size: CGSize(width: 402, height: 874), radius: 62, models: ["iPhone17,1", "iPhone18,1", "iPhone18,3", "iPhone19,2"]),
    Profile(size: CGSize(width: 440, height: 956), radius: 62, models: ["iPhone17,2", "iPhone18,2", "iPhone19,3", "iPhone19,7"]),
    Profile(size: CGSize(width: 420, height: 912), radius: 62, models: ["iPhone18,4"]),
    Profile(size: CGSize(width: 466, height: 678), radius: 0, models: ["iPhone19,4"])
  ]
  let portrait = CGSize(width: min(size.width, size.height), height: max(size.width, size.height))
  let extended = CGSize(width: portrait.width, height: portrait.height + 20)
  let model: String?
  #if targetEnvironment(simulator)
  model = ProcessInfo.processInfo.environment["SIMULATOR_MODEL_IDENTIFIER"]
  #else
  var info = utsname()
  model = uname(&info) == 0 ? withUnsafeBytes(of: &info.machine) { String(decoding: $0.prefix(while: { $0 != 0 }), as: UTF8.self) } : nil
  #endif
  if let model, let profile = profiles.first(where: { $0.models.contains(model) && $0.deviceScale == scale && ($0.size == portrait || $0.size == extended) }) { return profile.radius }
  if let profile = profiles.first(where: { ($0.size == portrait || $0.size == extended) && (!homeIndicator || $0.radius > 0) }) {
    if profile.size == CGSize(width: 375, height: 812), profile.radius == 39, statusBarHeight == 47 { return 53 + pixel }
    if profile.size == CGSize(width: 414, height: 896), profile.radius == 39, scale == 2 { return 41.5 }
    return profile.radius
  }
  if portrait.width >= 1024 || portrait.height >= 1024 { return 0 }
  return homeIndicator ? 39 : 0
}
