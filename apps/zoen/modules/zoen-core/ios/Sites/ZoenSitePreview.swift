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
  private var initialNavigation: WKNavigation?

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    clipsToBounds = true
    accessibilityLabel = "Site preview"
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
    initialNavigation = nil
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
    initialNavigation = view.loadHTMLString(ZoenSitePreviewPolicy.document(html), baseURL: nil)
  }

  func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction,
    decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
    // A nil target is a popup. All user links, forms, fragments, meta refreshes
    // and subsequent navigations are blocked, including about: and data:.
    let initial = initialNavigation != nil && navigationAction.navigationType == .other
      && navigationAction.targetFrame?.isMainFrame == true
      && navigationAction.request.url?.absoluteString == "about:blank"
    decisionHandler(initial ? .allow : .cancel)
  }

  func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
    initialNavigation = nil
  }

  func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration,
    for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? { nil }
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
