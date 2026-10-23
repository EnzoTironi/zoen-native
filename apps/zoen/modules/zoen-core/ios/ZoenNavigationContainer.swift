import ExpoModulesCore
import UIKit

/// Register the actual floating controls with UIKit's scroll-edge effect, as
/// native navigation/tab bars do. UIKit owns the blur, shape and accessibility.
@MainActor final class ZoenNavigationContainer: ZoenComposerGlassHost {
  override var contentHost: UIView { self }
  private let edge = UIScrollEdgeElementContainerInteraction()
  var scrollViewTag = 0

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    edge.edge = .top
    addInteraction(edge)
    isAccessibilityElement = false
  }

  func bind() {
    guard window != nil, scrollViewTag > 0,
          let root = appContext?.findView(withTag: scrollViewTag, ofType: UIView.self),
          root.window === window, let scroll = scrollView(in: root) else {
      edge.scrollView = nil
      return
    }
    scroll.topEdgeEffect.isHidden = false
    scroll.topEdgeEffect.style = .automatic
    edge.scrollView = scroll
  }

  private func scrollView(in root: UIView) -> UIScrollView? {
    if let scroll = root as? UIScrollView { return scroll }
    // Search only the explicitly supplied native scroll node, never the window
    // or another conversation. Fabric can place a host around UIScrollView.
    for child in root.subviews {
      if let scroll = scrollView(in: child) { return scroll }
    }
    return nil
  }

  override func didMoveToWindow() {
    super.didMoveToWindow()
    bind()
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    bind()
  }
}
