import Foundation

enum ZoenSitePreviewPolicy {
  // Placed before any untrusted bytes; later CSP declarations can only restrict.
  // No injected bridge or base URL. Remote assets belong to the explicit public
  // site, not this private source preview.
  static func document(_ html: String) -> String {
    let csp = "default-src 'none'; script-src 'none'; connect-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:; frame-src 'none'; object-src 'none'; form-action 'none'; base-uri 'none'"
    return "<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"\(csp)\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"></head><body>\(html)</body></html>"
  }
}
