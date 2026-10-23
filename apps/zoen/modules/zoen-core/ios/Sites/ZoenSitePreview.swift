import ExpoModulesCore
import WebKit

/// Static, disposable preview only. No account bridge, shared cookies, scripts,
/// local file access, external requests, popup, link, or authenticated session.
final class ZoenSitePreview: ExpoView, WKNavigationDelegate, WKUIDelegate {
  var html = ""
  var ownerKey = ""
  var active = false
  private var rendered: String?
  private var web: WKWebView?
  private var navigation = ZoenSitePreviewPolicy.Navigation()

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    clipsToBounds = true
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    web?.frame = bounds
  }

  override func didMoveToWindow() {
    super.didMoveToWindow()
    if window == nil { clear() } else { updatePreview() }
  }

  private func clear() {
    web?.stopLoading()
    web?.navigationDelegate = nil
    web?.uiDelegate = nil
    web?.removeFromSuperview()
    web = nil
    rendered = nil
    navigation = .init()
  }

  func updatePreview() {
    guard active, !ownerKey.isEmpty, window != nil,
      !html.isEmpty, html.utf8.count <= 2 * 1024 * 1024 else {
      clear()
      return
    }
    let key = ownerKey + "\0" + html
    guard rendered != key else { return }
    clear()
    let configuration = WKWebViewConfiguration()
    configuration.websiteDataStore = .nonPersistent()
    configuration.userContentController = WKUserContentController()
    configuration.defaultWebpagePreferences.allowsContentJavaScript = false
    configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
    configuration.allowsInlineMediaPlayback = false
    configuration.mediaTypesRequiringUserActionForPlayback = .all
    let view = WKWebView(frame: bounds, configuration: configuration)
    view.navigationDelegate = self
    view.uiDelegate = self
    view.allowsLinkPreview = false
    view.isInspectable = false
    web = view
    rendered = key
    addSubview(view)
    navigation.begin()
    view.loadHTMLString(ZoenSitePreviewPolicy.document(html), baseURL: nil)
  }

  func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction,
    decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
    // A nil target is a popup. All user links, forms, fragments, meta refreshes
    // and subsequent navigations are blocked, including about: and data:.
    let initial = navigation.allow(url: navigationAction.request.url?.absoluteString,
      main: navigationAction.targetFrame?.isMainFrame == true,
      other: navigationAction.navigationType == .other)
    decisionHandler(initial ? .allow : .cancel)
  }

  func webView(_ webView: WKWebView, didFinish navigation: WKNavigation?) {
    self.navigation = .init()
  }

  func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration,
    for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? { nil }

  // iOS otherwise enables file-upload panels by default, even with scripts off.
  func webView(_ webView: WKWebView, runOpenPanelWith parameters: WKOpenPanelParameters,
    initiatedByFrame frame: WKFrameInfo,
    completionHandler: @escaping @MainActor @Sendable ([URL]?) -> Void) {
    completionHandler(nil)
  }
}

func zoenSitePreviewDefinition() -> ViewDefinition<ZoenSitePreview> {
  View(ZoenSitePreview.self) {
    ViewName("SitePreview")
    Prop("html") { (view: ZoenSitePreview, value: String) in view.html = value }
    Prop("ownerKey") { (view: ZoenSitePreview, value: String) in view.ownerKey = value }
    Prop("active") { (view: ZoenSitePreview, value: Bool) in view.active = value }
    OnViewDidUpdateProps { view in view.updatePreview() }
  }
}
