import SwiftUI
import WebKit
import RodaCore

/// # Host de MCP Apps (SEP-1865, versão 2026-01-26)
///
/// Cada mini-app é um recurso `ui://` (`text/html;profile=mcp-app`) servido pelo núcleo
/// (o servidor MCP local). Aqui ele roda num `WKWebView` isolado:
///
/// - **Sem rede:** CSP restritiva do spec injetada no `<head>` + regra de bloqueio de
///   conteúdo para http(s)/ws(s)/file. Nenhum domínio é declarado, então nada sai.
/// - **Sem estado compartilhado:** `WKWebsiteDataStore.nonPersistent()` por cartão, sem
///   cookies nem storage do app, sem janelas novas, sem navegação.
/// - **Transporte:** o View fala JSON-RPC 2.0 com `window.parent.postMessage`, como num
///   iframe. Um script no início do documento troca `window.parent` por uma ponte para
///   `webkit.messageHandlers`, e as respostas voltam como eventos `message`.
/// - **Concessões:** todo `tools/call` vai para `RodaEngine.appCallTool`, que passa pelo
///   avaliador de Concessões. Irreversível ou externo volta como `needsConfirmation`, e só
///   roda depois da folha nativa de confirmação.
enum McpHost {
    static let protocolVersion = "2026-01-26"

    /// CSP padrão do spec quando o recurso não declara domínios (`ui.csp` omitido).
    static let csp = "default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; media-src 'self' data:; font-src 'self' data:; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'self'; form-action 'none'"

    static let shim = """
    (() => {
      const post = (m) => { try { window.webkit.messageHandlers.mcp.postMessage(JSON.stringify(m)); } catch (e) {} };
      const host = { postMessage: (m, _origin) => post(m) };
      try { Object.defineProperty(window, 'parent', { get: () => host, configurable: true }); } catch (e) { try { window.parent = host; } catch (_) {} }
      window.__rodaDeliver = (m) => window.dispatchEvent(new MessageEvent('message', { data: m }));
      window.open = () => null;
    })();
    """

    static let blockRules = """
    [{"trigger":{"url-filter":"^https?://"},"action":{"type":"block"}},
     {"trigger":{"url-filter":"^wss?://"},"action":{"type":"block"}},
     {"trigger":{"url-filter":"^file:"},"action":{"type":"block"}}]
    """

    @MainActor static var ruleList: WKContentRuleList?

    @MainActor static func withRules(_ done: @escaping (WKContentRuleList?) -> Void) {
        if let ruleList { return done(ruleList) }
        WKContentRuleListStore.default().compileContentRuleList(forIdentifier: "roda-mcp-no-network", encodedContentRuleList: blockRules) { list, _ in
            MainActor.assumeIsolated {
                ruleList = list
                done(list)
            }
        }
    }

    static func injectCSP(_ html: String) -> String {
        let meta = "<meta http-equiv=\"Content-Security-Policy\" content=\"\(csp)\">"
        if let r = html.range(of: "<head>", options: .caseInsensitive) {
            return html.replacingCharacters(in: r, with: "<head>" + meta)
        }
        return meta + html
    }

    static func symbol(for app: String) -> String {
        switch app {
        case "pet": "pawprint.fill"
        case "poll": "chart.bar.fill"
        case "countdown": "airplane"
        case "hike": "figure.hiking"
        default: "checklist"
        }
    }

    static func tint(for app: String) -> Color {
        switch app {
        case "pet": Color(hex: "#E8833A")
        case "poll": Color(hex: "#5B8DEF")
        case "countdown": Color(hex: "#E9846B")
        case "hike": Color(hex: "#3D7A28")
        default: Color(hex: "#3FB27F")
        }
    }
}

/// Pedido de confirmação nativa (ação irreversível ou externa vinda de um mini-app).
struct AppConfirmRequest: Identifiable {
    let id = UUID()
    let title: String
    let detail: String
    let appName: String
    let destructive: Bool
    var confirmLabel: String? = nil
    let decide: (Bool) -> Void
}

/// Ponte host ↔ View. Implementa: `ui/initialize`, `ui/notifications/initialized`,
/// `tools/call`, `resources/read`, `ui/open-link`, `ui/message`,
/// `ui/request-display-mode`, `ui/update-model-context`, `ping`,
/// `notifications/message`, `ui/notifications/size-changed`; e envia `tool-input`,
/// `tool-result`, `host-context-changed` e `ui/resource-teardown`.
@MainActor
final class McpAppBridge: NSObject, WKScriptMessageHandler, WKNavigationDelegate, WKUIDelegate {
    let itemId: String
    let model: AppModel
    var displayMode: String
    var dark: Bool
    var width: CGFloat = 360
    var onHeight: (CGFloat) -> Void = { _ in }
    var onFullscreen: () -> Void = {}
    var onClose: () -> Void = {}
    var edgeToEdge = false
    weak var webView: WKWebView?
    private var manifest: MiniAppManifest?
    private var grantedHosts: [String] = []
    private var openedAt = Date()
    private var poolHit = false
    private var initialized = false
    private var sentVersion: UInt32 = 0
    private var nextHostId = 1_000_000

    init(itemId: String, model: AppModel, displayMode: String, dark: Bool) {
        self.itemId = itemId
        self.model = model
        self.displayMode = displayMode
        self.dark = dark
    }

    // MARK: montar

    func makeWebView() -> WKWebView {
        openedAt = Date()
        let (wv, hit) = MiniAppWebPool.shared.take(for: self)
        poolHit = hit
        wv.navigationDelegate = self
        wv.uiDelegate = self
        #if os(iOS)
        wv.isOpaque = false
        wv.backgroundColor = .clear
        wv.scrollView.backgroundColor = .clear
        wv.scrollView.isScrollEnabled = displayMode == "fullscreen" && !edgeToEdge
        wv.scrollView.bounces = false
        wv.scrollView.contentInsetAdjustmentBehavior = .never
        #else
        wv.setValue(false, forKey: "drawsBackground")
        #endif
        webView = wv
        prepare(wv)
        return wv
    }

    /// Integrity, then network consent, then the content rules for exactly the hosts you
    /// allowed, then load.
    private func prepare(_ wv: WKWebView) {
        guard let item = try? model.core.item(itemId: itemId), let app = item.app,
              let res = try? model.core.readAppResource(uri: app.resourceUri) else { return }
        manifest = MiniAppManifest.decode(res.manifestJson)
        if let manifest, !manifest.verifies(res.text) {
            Haptics.warning()
            wv.loadHTMLString("<!doctype html><meta name=viewport content='width=device-width'><body style='font:15px -apple-system;padding:32px;color:#8a2a1a'>This mini-app didn’t match its signed bundle, so Zoen didn’t open it.</body>", baseURL: nil)
            return
        }
        let wanted = manifest?.allowedDomains ?? []
        grantedHosts = wanted.filter { model.core.appDeviceAllowed(itemId: itemId, capability: "net:" + $0) }
        let askKey = "RodaNetAsked.\(itemId)"
        if let manifest, !wanted.isEmpty, grantedHosts.isEmpty, !UserDefaults.standard.bool(forKey: askKey) {
            let space = model.space(item.spaceId)?.title ?? ""
            model.consent = ConsentRequest(capability: "net:" + wanted.joined(separator: ", "), appName: manifest.name, spaceTitle: space,
                                           purpose: manifest.networkPurpose ?? "") { [weak self, weak wv] d in
                guard let self, let wv else { return }
                UserDefaults.standard.set(true, forKey: askKey)
                if d != .deny {
                    for h in wanted { _ = try? self.model.core.grantAppDevice(itemId: self.itemId, capability: "net:" + h, purpose: manifest.networkPurpose ?? "", always: d == .always) }
                    self.grantedHosts = wanted
                }
                self.model.refresh()
                self.compileAndLoad(wv, res.text, version: item.version)
            }
            return
        }
        compileAndLoad(wv, res.text, version: item.version)
    }

    private func compileAndLoad(_ wv: WKWebView, _ html: String, version: UInt32) {
        let hosts = grantedHosts
        let csp = manifest == nil ? McpHost.csp : MiniAppSandbox.csp(allowing: hosts)
        MiniAppSandbox.rules(allowing: hosts) { [weak self, weak wv] list in
            guard let self, let wv else { return }
            wv.configuration.userContentController.removeAllContentRuleLists()
            if let list { wv.configuration.userContentController.add(list) }
            self.sentVersion = version
            wv.loadHTMLString(MiniAppSandbox.injectCSP(html, csp), baseURL: nil)
        }
    }

    func teardown() {
        guard initialized else { return }
        nextHostId += 1
        deliver(["jsonrpc": "2.0", "id": nextHostId, "method": "ui/resource-teardown", "params": ["reason": "card left the screen"]])
    }

    // MARK: sandbox: nada de navegar nem abrir janelas

    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction, decisionHandler: @escaping @MainActor @Sendable (WKNavigationActionPolicy) -> Void) {
        let url = action.request.url?.absoluteString ?? ""
        decisionHandler(url == "about:blank" || url.isEmpty ? .allow : .cancel)
    }

    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? { nil }

    // MARK: View → host

    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        guard let s = message.body as? String, let data = s.data(using: .utf8),
              let msg = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
        let method = msg["method"] as? String
        let params = msg["params"] as? [String: Any] ?? [:]
        if let id = msg["id"], let method {
            handleRequest(id: id, method: method, params: params)
        } else if let method {
            handleNotification(method, params)
        }
        // Respostas do View (ex.: à teardown) não precisam de tratamento.
    }

    private func handleNotification(_ method: String, _ params: [String: Any]) {
        switch method {
        case "ui/notifications/initialized":
            initialized = true
            sendToolData(includeInput: true)
            let ms = Int(Date().timeIntervalSince(openedAt) * 1000)
            let app = (try? model.core.item(itemId: itemId))?.app?.appId ?? "?"
            MiniAppPerf.record("open \(app) \(displayMode) \(ms) ms · \(poolHit ? "prewarmed" : "cold view") · \(manifest.map { "\($0.bytes / 1024) KB bundle" } ?? "inline html")")
        case "zoen/haptics":
            switch params["kind"] as? String {
            case "success": Haptics.action()
            case "warning": Haptics.warning()
            case "select": Haptics.selectionTick()
            default: Haptics.selectionTick()
            }
        case "ui/notifications/size-changed":
            if let h = params["height"] as? Double { onHeight(CGFloat(h)) }
        case "notifications/message":
            print("[MCP App \(itemId)]", params["data"] ?? params)
        default:
            break
        }
    }

    private func handleRequest(id: Any, method: String, params: [String: Any]) {
        switch method {
        case "ui/initialize":
            respond(id, result: initializeResult())
        case "ping":
            respond(id, result: [:])
        case "tools/call":
            let name = params["name"] as? String ?? ""
            let args = params["arguments"] ?? [:]
            callTool(id: id, name: name, args: args, confirmed: false)
        case "resources/read":
            let uri = params["uri"] as? String ?? ""
            if let res = try? model.core.readAppResource(uri: uri) {
                respond(id, result: ["contents": [["uri": res.uri, "mimeType": res.mimeType, "text": res.text]]])
            } else {
                respondError(id, code: -32002, message: "Resource not found")
            }
        case "ui/open-link":
            guard let s = params["url"] as? String, let url = URL(string: s), ["https", "http", "mailto"].contains(url.scheme ?? "") else {
                return respondError(id, code: -32000, message: "Invalid URL")
            }
            confirm(title: String(localized: "Open link outside Zoen?"), detail: String(localized: "The mini-app wants to open \(url.host() ?? s) in the browser."), destructive: false) { [weak self] ok in
                guard let self else { return }
                if ok {
                    self.model.openExternal(url)
                    self.respond(id, result: [:])
                } else {
                    self.respondError(id, code: -32000, message: "Link opening denied by user")
                }
            }
        case "ui/message":
            let content = params["content"] as? [String: Any]
            let text = (content?["text"] as? String) ?? ""
            guard !text.isEmpty, let item = try? model.core.item(itemId: itemId) else {
                return respondError(id, code: -32000, message: "Invalid message format")
            }
            model.perform { try model.core.sendMessage(spaceId: item.spaceId, text: text) }
            respond(id, result: [:])
        case "ui/request-display-mode":
            let mode = params["mode"] as? String ?? "inline"
            if mode == "fullscreen" && displayMode != "fullscreen" { onFullscreen() }
            if mode == "inline" && displayMode == "fullscreen" { onClose() }
            respond(id, result: ["mode": mode == "fullscreen" ? "fullscreen" : displayMode])
        case "zoen/widget/set-snapshot":
            let snap = params["snapshot"].flatMap { try? JSONSerialization.data(withJSONObject: $0) }.flatMap { String(data: $0, encoding: .utf8) }
            guard let snap, WidgetSnapshot.decode(snap) != nil else {
                return respondError(id, code: -32602, message: "Snapshot doesn’t match the widget schema")
            }
            WidgetOverrides.byItem[itemId] = snap
            model.refresh()
            respond(id, result: ["accepted": true])
        case "zoen/ai/ask":
            respondError(id, code: -32601, message: "The group’s agent doesn’t take prompts from mini-apps in this build")
        case let m where m.hasPrefix("zoen/native/"):
            native(id: id, capability: String(m.dropFirst("zoen/native/".count)), params: params)
        case "ui/update-model-context":
            model.appModelContext[itemId] = params
            respond(id, result: [:])
        default:
            respondError(id, code: -32601, message: "Method not found: \(method)")
        }
    }

    /// Photos picked through `zoen.native.photos.pick` come back as tokens; they turn into
    /// pixels in shared state only after you confirm "Share with group" here.
    private func callTool(id: Any, name: String, args: Any, confirmed: Bool) {
        let strings = Self.strings(in: args)
        if strings.contains(where: { $0.hasPrefix("data:image") }) {
            return respondError(id, code: -32602, message: "Photos reach the group only through the photo picker and “Share with group”")
        }
        let tokens = strings.filter { $0.hasPrefix("zoen-photo:") }
        if !tokens.isEmpty {
            guard tokens.allSatisfy({ PhotoVault.resolve($0, item: itemId) != nil }) else {
                return respondError(id, code: -32602, message: "Unknown photo")
            }
            let item = try? model.core.item(itemId: itemId)
            let space = item.flatMap { model.space($0.spaceId)?.title } ?? ""
            let appName = item?.app?.name ?? String(localized: "The mini-app")
            confirm(title: String(localized: "Share \(tokens.count) photo(s) with \(space)?"),
                    detail: String(localized: "Everyone in the chat will see them in \(appName). Location data was removed."),
                    destructive: false, appName: appName, confirmLabel: String(localized: "Share with group")) { [weak self] ok in
                guard let self else { return }
                guard ok else { Haptics.dismiss(); return self.respondError(id, code: -32000, message: String(localized: "You didn’t share them.")) }
                let resolved = Self.replacingTokens(in: args) { PhotoVault.resolve($0, item: self.itemId) ?? $0 }
                self.runTool(id: id, name: name, args: resolved, confirmed: confirmed)
            }
            return
        }
        runTool(id: id, name: name, args: args, confirmed: confirmed)
    }

    private static func strings(in v: Any) -> [String] {
        if let s = v as? String { return [s] }
        if let a = v as? [Any] { return a.flatMap { strings(in: $0) } }
        if let d = v as? [String: Any] { return d.values.flatMap { strings(in: $0) } }
        return []
    }

    private static func replacingTokens(in v: Any, _ f: (String) -> String) -> Any {
        if let s = v as? String { return s.hasPrefix("zoen-photo:") ? f(s) : s }
        if let a = v as? [Any] { return a.map { replacingTokens(in: $0, f) } }
        if let d = v as? [String: Any] { return d.mapValues { replacingTokens(in: $0, f) } }
        return v
    }

    /// `zoen/native/*`: declared in the manifest → Zoen's consent sheet (unless a Grant
    /// already covers it) → the iOS prompt or picker → result. Every grant is a signed event.
    private func native(id: Any, capability cap: String, params: [String: Any]) {
        guard let manifest, let purpose = manifest.purpose(cap) else {
            return respondError(id, code: -32003, message: "\(cap) isn’t declared in this mini-app’s manifest")
        }
        guard NativeCapabilities.available.contains(cap) else {
            return respondError(id, code: -32601, message: cap == "health.steps" ? "Health isn’t connected in this build" : "This device can’t do \(cap)")
        }
        let run = { [weak self] in
            guard let self else { return }
            Task { @MainActor in
                do {
                    let r = try await NativeCapabilities.perform(cap, params: params, item: self.itemId)
                    self.respond(id, result: r)
                } catch {
                    self.respondError(id, code: -32000, message: error.localizedDescription)
                }
            }
        }
        if model.core.appDeviceAllowed(itemId: itemId, capability: cap) { return run() }
        let item = try? model.core.item(itemId: itemId)
        model.consent = ConsentRequest(capability: cap, appName: manifest.name, spaceTitle: item.flatMap { model.space($0.spaceId)?.title } ?? "", purpose: purpose) { [weak self] d in
            guard let self else { return }
            guard d != .deny else { return self.respondError(id, code: -32000, message: String(localized: "You didn’t allow it.")) }
            do {
                _ = try self.model.core.grantAppDevice(itemId: self.itemId, capability: cap, purpose: purpose, always: d == .always)
                self.model.refresh()
                // The sheet slides away before iOS (or the picker) comes up.
                Task { @MainActor in try? await Task.sleep(for: .milliseconds(450)); run() }
            } catch {
                self.respondError(id, code: -32000, message: (error as? CoreError)?.message ?? error.localizedDescription)
            }
        }
    }

    private func runTool(id: Any, name: String, args: Any, confirmed: Bool) {
        let argsJSON = (try? JSONSerialization.data(withJSONObject: args)).flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
        do {
            let out = try model.core.appCallTool(itemId: itemId, tool: name, argsJson: argsJSON, confirmed: confirmed)
            switch out.status {
            case .done:
                let result = (try? JSONSerialization.jsonObject(with: Data(out.resultJson.utf8))) ?? [:]
                let isError = (result as? [String: Any])?["isError"] as? Bool ?? false
                if isError { Haptics.dismiss() } else { Haptics.action() }
                if let v = out.item?.version { sentVersion = v }
                respond(id, result: result)
                model.refresh()
                if name == "hike_decide" && !isError {
                    Task { @MainActor in try? await Task.sleep(for: .seconds(1)); self.model.planHikeIfReady(self.itemId) }
                }
            case .needsConfirmation:
                Haptics.warning()
                let appName = (try? model.core.item(itemId: itemId))?.app?.name ?? String(localized: "The mini-app")
                confirm(title: out.confirmTitle ?? name, detail: out.confirmDetail ?? out.message, destructive: true, appName: appName) { [weak self] ok in
                    guard let self else { return }
                    if ok {
                        self.runTool(id: id, name: name, args: args, confirmed: true)
                    } else {
                        Haptics.dismiss()
                        self.respondError(id, code: -32000, message: String(localized: "You didn’t confirm."))
                    }
                }
            case .denied:
                Haptics.dismiss()
                respondError(id, code: -32000, message: out.message)
            }
        } catch {
            Haptics.dismiss()
            respondError(id, code: -32000, message: (error as? CoreError)?.message ?? error.localizedDescription)
        }
    }

    private func confirm(title: String, detail: String, destructive: Bool, appName: String = String(localized: "The mini-app"), confirmLabel: String? = nil, decide: @escaping (Bool) -> Void) {
        model.appConfirm = AppConfirmRequest(title: title, detail: detail, appName: appName, destructive: destructive, confirmLabel: confirmLabel, decide: decide)
    }

    // MARK: host → View

    private func initializeResult() -> [String: Any] {
        [
            "protocolVersion": McpHost.protocolVersion,
            "hostInfo": ["name": "zoen", "version": "0.3.0"],
            "hostCapabilities": [
                "openLinks": [String: Any](),
                "serverTools": [String: Any](),
                "serverResources": [String: Any](),
                "logging": [String: Any](),
                "sandbox": ["csp": ["connectDomains": grantedHosts.map { "https://\($0)" }], "permissions": [String: Any]()],
                // Zoen extensions: other MCP hosts don't send this, so `capabilities.has()`
                // is false there and the app falls back.
                "experimental": ["zoen": [
                    "native": NativeCapabilities.available.filter { manifest?.purpose($0) != nil },
                    "services": ["state", "members", "haptics", "share", "widget", "theme"] + grantedHosts.map { "net:" + $0 },
                    "me": members().first { ($0["isMe"] as? Bool) == true } ?? [:],
                ] as [String: Any]],
            ],
            "hostContext": hostContext(),
        ]
    }

    func hostContext() -> [String: Any] {
        var ctx: [String: Any] = [
            "theme": dark ? "dark" : "light",
            "styles": ["variables": styleVariables()],
            "displayMode": displayMode,
            "availableDisplayModes": ["inline", "fullscreen"],
            "containerDimensions": displayMode == "fullscreen" ? ["width": Double(width)] : ["width": Double(width), "maxHeight": 640.0],
            "locale": AppLocale.tag,
            "timeZone": TimeZone.current.identifier,
            "userAgent": "Zoen/0.3",
            "deviceCapabilities": ["touch": true, "hover": false],
            "safeAreaInsets": ["top": 0, "right": 0, "bottom": 0, "left": 0],
            "zoen": ["members": members()],
        ]
        #if os(iOS)
        ctx["platform"] = "mobile"
        #else
        ctx["platform"] = "desktop"
        #endif
        if let item = try? model.core.item(itemId: itemId), let app = item.app,
           let spec = model.core.appSpecs().first(where: { $0.id == app.appId }),
           let tool = spec.tools.first(where: { $0.visibility == ["model"] }) {
            let schema = (try? JSONSerialization.jsonObject(with: Data(tool.inputSchemaJson.utf8))) ?? [:]
            ctx["toolInfo"] = ["tool": ["name": tool.name, "description": tool.description, "inputSchema": schema, "_meta": ["ui": ["resourceUri": spec.resourceUri, "visibility": tool.visibility]]]]
        }
        return ctx
    }

    /// People in the mini-app's Space (agents excluded), you first.
    private func members() -> [[String: Any]] {
        guard let item = try? model.core.item(itemId: itemId), let space = model.space(item.spaceId) else { return [] }
        var people = space.members.filter { $0.kind != .agent }
        if let me = model.me, !people.contains(where: { $0.id == me.id }) { people.insert(me, at: 0) }
        people.sort { $0.isMe && !$1.isMe }
        return people.map { ["id": $0.id, "name": $0.name, "initials": $0.initials, "color": $0.tintHex, "isMe": $0.isMe] }
    }

    private func styleVariables() -> [String: String] {
        dark
            ? ["--color-background-primary": "#1F1F22", "--color-background-secondary": "#2A2A2E", "--color-text-primary": "#F2F2F0",
               "--color-text-secondary": "#A1A1A6", "--color-border-primary": "#3A3A3F", "--font-sans": "-apple-system, system-ui, sans-serif"]
            : ["--color-background-primary": "#FBFAF7", "--color-background-secondary": "#F1EFEA", "--color-text-primary": "#1C1C1E",
               "--color-text-secondary": "#6B6B70", "--color-border-primary": "#E4E1DA", "--font-sans": "-apple-system, system-ui, sans-serif"]
    }

    private func sendToolData(includeInput: Bool) {
        guard let item = try? model.core.item(itemId: itemId), let app = item.app else { return }
        if includeInput {
            deliver(["jsonrpc": "2.0", "method": "ui/notifications/tool-input", "params": ["arguments": [String: Any]()]])
        }
        let view = (try? JSONSerialization.jsonObject(with: Data(app.viewJson.utf8))) ?? [:]
        deliver(["jsonrpc": "2.0", "method": "ui/notifications/tool-result",
                 "params": ["content": [["type": "text", "text": app.headline]], "structuredContent": view, "isError": false]])
        sentVersion = item.version
    }

    /// O Item mudou por fora (outro cartão, outra pessoa): reenvia o estado. O spec manda
    /// `tool-result` por execução de ferramenta; reenviar a cada versão é uma extensão
    /// nossa para estado compartilhado ao vivo.
    func itemMaybeChanged() {
        guard initialized, let item = try? model.core.item(itemId: itemId), item.version != sentVersion else { return }
        sendToolData(includeInput: false)
    }

    func contextChanged(dark: Bool, width: CGFloat) {
        let changed = dark != self.dark
        self.dark = dark
        self.width = width
        guard initialized, changed else { return }
        deliver(["jsonrpc": "2.0", "method": "ui/notifications/host-context-changed", "params": ["theme": dark ? "dark" : "light", "styles": ["variables": styleVariables()]]])
    }

    private func respond(_ id: Any, result: Any) {
        deliver(["jsonrpc": "2.0", "id": id, "result": result])
    }

    private func respondError(_ id: Any, code: Int, message: String) {
        deliver(["jsonrpc": "2.0", "id": id, "error": ["code": code, "message": message]])
    }

    private func deliver(_ msg: [String: Any]) {
        guard let webView, let data = try? JSONSerialization.data(withJSONObject: msg), let json = String(data: data, encoding: .utf8) else { return }
        webView.evaluateJavaScript("window.__rodaDeliver && window.__rodaDeliver(\(json));", completionHandler: nil)
    }
}

/// Evita o ciclo de retenção WKUserContentController → handler.
private final class WeakHandler: NSObject, WKScriptMessageHandler {
    weak var target: (any WKScriptMessageHandler)?
    init(_ target: any WKScriptMessageHandler) { self.target = target }
    func userContentController(_ uc: WKUserContentController, didReceive message: WKScriptMessage) {
        target?.userContentController(uc, didReceive: message)
    }
}

// MARK: - SwiftUI

@MainActor
struct McpAppWebView {
    @Environment(AppModel.self) var model
    @Environment(\.colorScheme) var scheme
    let itemId: String
    var displayMode = "inline"
    @Binding var height: CGFloat
    var onFullscreen: () -> Void = {}
    var onClose: () -> Void = {}
    /// The app draws its own chrome edge to edge (no page scroll; it scrolls inside).
    var edgeToEdge = false

    func makeCoordinator() -> McpAppBridge {
        let b = McpAppBridge(itemId: itemId, model: model, displayMode: displayMode, dark: scheme == .dark)
        b.edgeToEdge = edgeToEdge
        return b
    }

    fileprivate func wire(_ bridge: McpAppBridge) {
        bridge.onHeight = { h in
            Task { @MainActor in
                let clamped = min(max(h, 120), displayMode == "fullscreen" ? 4000 : 640)
                if abs(clamped - height) > 1 { withAnimation(.snappy) { height = clamped } }
            }
        }
        bridge.onFullscreen = onFullscreen
        bridge.onClose = onClose
    }

    fileprivate func update(_ bridge: McpAppBridge) {
        bridge.contextChanged(dark: scheme == .dark, width: bridge.width)
        bridge.itemMaybeChanged()
    }
}

#if os(iOS)
extension McpAppWebView: UIViewRepresentable {
    func makeUIView(context: Context) -> WKWebView {
        wire(context.coordinator)
        return context.coordinator.makeWebView()
    }
    func updateUIView(_ view: WKWebView, context: Context) {
        context.coordinator.width = view.bounds.width > 0 ? view.bounds.width : context.coordinator.width
        _ = model.revision
        update(context.coordinator)
    }
    static func dismantleUIView(_ view: WKWebView, coordinator: McpAppBridge) {
        coordinator.teardown()
    }
}
#else
extension McpAppWebView: NSViewRepresentable {
    func makeNSView(context: Context) -> WKWebView {
        wire(context.coordinator)
        return context.coordinator.makeWebView()
    }
    func updateNSView(_ view: WKWebView, context: Context) {
        _ = model.revision
        update(context.coordinator)
    }
    static func dismantleNSView(_ view: WKWebView, coordinator: McpAppBridge) {
        coordinator.teardown()
    }
}
#endif
