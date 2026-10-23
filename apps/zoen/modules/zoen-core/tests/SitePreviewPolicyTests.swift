import Foundation

@main struct SitePreviewPolicyTests {
  static func main() {
    var policy = ZoenSitePreviewPolicy.Navigation()
    precondition(!policy.allow(url: "about:blank", main: true, other: true))
    policy.begin()
    for url in ["https://remote.test", "file:///private/secret", "data:text/html,hello", "about:blank#fragment"] {
      precondition(!policy.allow(url: url, main: true, other: true))
    }
    precondition(!policy.allow(url: "about:blank", main: false, other: true))
    precondition(!policy.allow(url: "about:blank", main: true, other: false))
    precondition(policy.allow(url: "about:blank", main: true, other: true))
    // Even a meta refresh to about:blank cannot reuse initial admission while
    // the first document is still loading.
    precondition(!policy.allow(url: "about:blank", main: true, other: true))
    policy = .init()
    precondition(!policy.allow(url: "about:blank", main: true, other: true))
    let html = "<meta http-equiv='Content-Security-Policy' content='default-src *'><script>PRIVATE_CANARY</script>"
    let document = ZoenSitePreviewPolicy.document(html)
    precondition(document.range(of: "default-src 'none'")!.lowerBound < document.range(of: "PRIVATE_CANARY")!.lowerBound)
    for directive in ["script-src 'none'", "connect-src 'none'", "frame-src 'none'", "form-action 'none'", "base-uri 'none'"] {
      precondition(document.contains(directive))
    }
    precondition(document.hasSuffix(html + "</body></html>"))
    print("PASS actual Site preview initial-only navigation, retirement and restrictive prefix")
  }
}
