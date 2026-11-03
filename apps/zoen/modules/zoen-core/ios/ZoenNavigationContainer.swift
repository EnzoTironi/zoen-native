import ExpoModulesCore
import UIKit

/// Register the actual floating controls with UIKit's scroll-edge effect, as
/// native navigation/tab bars do. UIKit owns the blur, shape and accessibility.
@MainActor final class ZoenNavigationContainer: ZoenComposerGlassHost {
  override var contentHost: UIView { self }
  private let edge = UIScrollEdgeElementContainerInteraction()
  let onMetricsChanged = EventDispatcher()
  var scrollViewTag = 0 { didSet {
    if oldValue != scrollViewTag { retireEdge(); retireMetrics() }
  } }
  var metricsBinding = "" { didSet {
    if oldValue != metricsBinding { retireEdge(); retireMetrics() }
  } }
  private weak var metricsScroll: UIScrollView?
  private var observations: [NSKeyValueObservation] = []
  private var metricsState = ZoenScrollMetricsState()
  private var metricsScheduled = false

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    edge.edge = .top
    isAccessibilityElement = false
  }

  func bind() {
    guard window != nil, scrollViewTag > 0,
          let root = appContext?.findView(withTag: scrollViewTag, ofType: UIView.self),
          root.window === window, let scroll = scrollView(in: root) else {
      retireMetrics()
      retireEdge()
      return
    }
    scroll.topEdgeEffect.isHidden = false
    scroll.topEdgeEffect.style = .soft
    // The title host is only one navigation item. Register its public bar
    // ancestor so Back and right items also contribute to UIKit's edge shape.
    if let bar = navigationBar(), bar.window === window {
      if edge.view !== bar {
        retireEdge()
        bar.addInteraction(edge)
      }
      edge.scrollView = scroll
    } else {
      retireEdge()
    }
    guard !metricsBinding.isEmpty else { return }
    if metricsScroll !== scroll {
      retireMetrics()
      metricsScroll = scroll
      observations = [
        scroll.observe(\.contentInset, options: [.new]) { [weak self] _, _ in
          MainActor.assumeIsolated { self?.scheduleMetrics() }
        },
        scroll.observe(\.contentOffset, options: [.new]) { [weak self] _, _ in
          MainActor.assumeIsolated { self?.scheduleMetrics() }
        },
        scroll.observe(\.bounds, options: [.new]) { [weak self] _, _ in
          MainActor.assumeIsolated { self?.scheduleMetrics() }
        },
        scroll.observe(\.contentSize, options: [.new]) { [weak self] _, _ in
          MainActor.assumeIsolated { self?.scheduleMetrics() }
        }
      ]
      scheduleMetrics()
    }
  }

  private func navigationBar() -> UINavigationBar? {
    var candidate = superview
    while let view = candidate {
      if let bar = view as? UINavigationBar { return bar }
      candidate = view.superview
    }
    return nil
  }

  private func retireEdge() {
    edge.scrollView = nil
    edge.view?.removeInteraction(edge)
  }

  private func retireMetrics() {
    metricsState.retire()
    observations.forEach { $0.invalidate() }
    observations.removeAll()
    metricsScroll = nil
    metricsScheduled = false
  }

  private func values(_ scroll: UIScrollView) -> [Double] {
    [scroll.contentOffset.y, scroll.contentSize.height, scroll.bounds.height,
     scroll.contentInset.top, scroll.contentInset.bottom].map(Double.init)
  }

  /// An explicit read also completes an unchanged-offset search. It observes
  /// UIKit on main; it never sends a synthetic scroll or borrows RN's delegate.
  func sampleScroll(_ binding: String) -> [String: Any]? {
    bind()
    guard !binding.isEmpty, binding == metricsBinding, let scroll = metricsScroll,
          scroll.window === window, window != nil else { return nil }
    let current = values(scroll)
    return metricsState.packet(binding: binding, values: current, onlyChanged: false)
  }

  private func scheduleMetrics() {
    guard !metricsScheduled, let scroll = metricsScroll, !metricsBinding.isEmpty else { return }
    metricsScheduled = true
    let epoch = metricsState.epoch
    DispatchQueue.main.async { [weak self, weak scroll] in
      guard let self, epoch == self.metricsState.epoch else { return }
      self.metricsScheduled = false
      guard let scroll, self.metricsScroll === scroll, self.window != nil,
            scroll.window === self.window else { return }
      let current = self.values(scroll)
      guard let result = self.metricsState.packet(binding: self.metricsBinding, values: current, onlyChanged: true) else { return }
      self.onMetricsChanged(result)
    }
  }

  override func willMove(toWindow newWindow: UIWindow?) {
    if newWindow !== window {
      retireEdge()
      retireMetrics()
    }
    super.willMove(toWindow: newWindow)
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

func zoenNavigationContainerDefinition() -> ViewDefinition<ZoenNavigationContainer> {
  View(ZoenNavigationContainer.self) {
    ViewName("NavigationContainer")
    Events("onMetricsChanged")
    Prop("metricsBinding") { (view: ZoenNavigationContainer, value: String) in view.metricsBinding = value }
    AsyncFunction("sampleScroll") { (view: ZoenNavigationContainer, binding: String) async -> [String: Any]? in
      await view.sampleScroll(binding)
    }
    Prop("scrollViewTag") { (view: ZoenNavigationContainer, value: Int) in view.scrollViewTag = value }
    OnViewDidUpdateProps { (view: ZoenNavigationContainer) in view.bind() }
  }
}
