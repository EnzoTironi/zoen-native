import ExpoModulesCore
import UIKit

@MainActor extension ZoenMessageContextController {
  func animateEntry() {
    UIView.animate(withDuration: reduceMotion ? 0 : 0.2) { self.backdrop.alpha = 1 }
    actions.alpha = 1; reactionScroll.alpha = 1
    guard !reduceMotion else { return }
    if let source, pressedScale < 1 { spring(source.contentHost.layer, key: "transform.scale", from: pressedScale, to: 1) }
    spring(preview.layer, key: "position", from: NSValue(cgPoint: CGPoint(x: sourceFrame.midX, y: sourceFrame.midY)), to: NSValue(cgPoint: preview.center))
    for (bar, duration) in [(actions, 0.05), (reactionScroll, 0.15)] {
      let alpha = CABasicAnimation(keyPath: "opacity"); alpha.fromValue = 0; alpha.toValue = 1; alpha.duration = duration
      bar.layer.add(alpha, forKey: "context.opacity")
      spring(bar.layer, key: "transform.scale", from: 0.01, to: 1)
    }
    for button in reactionButtons {
      let center = button.convert(CGPoint(x: button.bounds.midX, y: button.bounds.midY), to: view)
      let distance = abs(center.x - sourceFrame.midX) / max(1, reactionScroll.bounds.width)
      let animation = CASpringAnimation(keyPath: "transform.scale")
      animation.mass = 5; animation.stiffness = 900; animation.damping = 104
      animation.fromValue = 0.01; animation.toValue = 1
      animation.duration = animation.settlingDuration; animation.speed = Float(animation.settlingDuration / 0.42)
      animation.beginTime = CACurrentMediaTime() + 0.01 + Double(distance) * 0.3
      animation.fillMode = .backwards
      button.layer.add(animation, forKey: "context.item")
    }
  }
  func spring(_ layer: CALayer, key: String, from: Any, to: Any) {
    let animation = CASpringAnimation(keyPath: key)
    animation.mass = 5; animation.stiffness = 900; animation.damping = 104; animation.initialVelocity = 0
    animation.fromValue = from; animation.toValue = to
    animation.duration = animation.settlingDuration; animation.speed = Float(animation.settlingDuration / 0.42)
    layer.add(animation, forKey: "context.\(key)")
  }
}
