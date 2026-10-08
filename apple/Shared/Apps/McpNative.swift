import SwiftUI
import WebKit
import CryptoKit
import CoreLocation
import EventKit
import RodaCore
#if os(iOS)
import PhotosUI
import UniformTypeIdentifiers
import ContactsUI
import EventKitUI
#endif

// MARK: - Manifest

/// What a bundled mini-app declares (written by `tools/miniapp-build`, served by the core
/// next to the HTML). The host trusts nothing that isn't here: a capability without a
/// purpose, or a network host that isn't listed, simply doesn't exist for the app.
struct MiniAppManifest: Decodable, Sendable {
    struct Cap: Decodable, Sendable { let id: String; let purpose: String }
    let id: String
    let name: String
    let capabilities: [Cap]
    let allowedDomains: [String]
    let networkPurpose: String?
    let linkDomains: [String]?
    let sha256: String
    let bytes: Int

    static func decode(_ s: String) -> MiniAppManifest? {
        guard !s.isEmpty else { return nil }
        return try? JSONDecoder().decode(Self.self, from: Data(s.utf8))
    }

    func purpose(_ cap: String) -> String? { capabilities.first { $0.id == cap }?.purpose }

    /// Bundle integrity: the HTML must hash to what the build tool recorded.
    func verifies(_ html: String) -> Bool {
        let digest = SHA256.hash(data: Data(html.utf8)).map { String(format: "%02x", $0) }.joined()
        return digest == sha256.lowercased()
    }
}

// MARK: - Sandbox policy per app

enum MiniAppSandbox {
    /// CSP for a bundled app. Workers may only come from `blob:` (MapLibre builds its worker
    /// that way); `connect-src`/`img-src` open up only for hosts you allowed.
    static func csp(allowing hosts: [String]) -> String {
        let net = hosts.map { "https://\($0)" }.joined(separator: " ")
        let connect = hosts.isEmpty ? "'none'" : net
        return "default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: \(net); media-src 'self' data:; font-src 'self' data:; connect-src \(connect); worker-src blob:; child-src blob:; frame-src 'none'; object-src 'none'; base-uri 'self'; form-action 'none'"
    }

    static func injectCSP(_ html: String, _ csp: String) -> String {
        let meta = "<meta http-equiv=\"Content-Security-Policy\" content=\"\(csp)\">"
        if let r = html.range(of: "<head>", options: .caseInsensitive) {
            return html.replacingCharacters(in: r, with: "<head>" + meta)
        }
        return meta + html
    }

    /// Content rules: block every http(s)/ws(s)/file load, then re-allow only the granted
    /// hosts (in the same list, so the order is guaranteed).
    @MainActor private static var lists: [String: WKContentRuleList] = [:]

    @MainActor static func rules(allowing hosts: [String], _ done: @escaping (WKContentRuleList?) -> Void) {
        let key = "zoen-net-" + (hosts.isEmpty ? "none" : hosts.sorted().joined(separator: "_"))
        if let l = lists[key] { return done(l) }
        var rules: [[String: Any]] = [
            ["trigger": ["url-filter": "^https?://"], "action": ["type": "block"]],
            ["trigger": ["url-filter": "^wss?://"], "action": ["type": "block"]],
            ["trigger": ["url-filter": "^file:"], "action": ["type": "block"]],
        ]
        for h in hosts {
            let escaped = NSRegularExpression.escapedPattern(for: h)
            rules.append(["trigger": ["url-filter": "^https://\(escaped)/"], "action": ["type": "ignore-previous-rules"]])
        }
        let json = String(data: (try? JSONSerialization.data(withJSONObject: rules)) ?? Data("[]".utf8), encoding: .utf8) ?? "[]"
        WKContentRuleListStore.default().compileContentRuleList(forIdentifier: key.replacingOccurrences(of: ".", with: "-"), encodedContentRuleList: json) { list, _ in
            MainActor.assumeIsolated {
                if let list { lists[key] = list }
                done(list)
            }
        }
    }
}

// MARK: - Prewarmed web views

/// Two web views kept warm (WebContent process launched, shim installed) so a mini-app
/// opens without paying process start-up. Each one is used once and thrown away: no state
/// carries over between apps.
@MainActor
final class MiniAppWebPool {
    static let shared = MiniAppWebPool()
    private var warm: [(WKWebView, RelayHandler)] = []
    private let size = 2

    func prewarm() {
        while warm.count < size {
            let relay = RelayHandler()
            let wv = Self.make(relay: relay)
            wv.loadHTMLString("<!doctype html><title>warm</title>", baseURL: nil)
            warm.append((wv, relay))
        }
    }

    /// A fresh view (from the pool when there is one) wired to `target`.
    func take(for target: any WKScriptMessageHandler) -> (WKWebView, hit: Bool) {
        defer { Task { @MainActor in try? await Task.sleep(for: .milliseconds(400)); self.prewarm() } }
        if !warm.isEmpty {
            let (wv, relay) = warm.removeFirst()
            relay.target = target
            return (wv, true)
        }
        let relay = RelayHandler()
        relay.target = target
        return (Self.make(relay: relay), false)
    }

    private static func make(relay: RelayHandler) -> WKWebView {
        let cfg = WKWebViewConfiguration()
        cfg.websiteDataStore = .nonPersistent()
        cfg.preferences.javaScriptCanOpenWindowsAutomatically = false
        #if os(iOS)
        cfg.dataDetectorTypes = []
        cfg.allowsInlineMediaPlayback = false
        #endif
        let uc = WKUserContentController()
        uc.addUserScript(WKUserScript(source: McpHost.shim, injectionTime: .atDocumentStart, forMainFrameOnly: true))
        uc.add(relay, name: "mcp")
        cfg.userContentController = uc
        return WKWebView(frame: .zero, configuration: cfg)
    }
}

/// Message handler whose target is set when the view is checked out (and held weakly, so
/// the controller → handler → bridge cycle never forms).
final class RelayHandler: NSObject, WKScriptMessageHandler {
    weak var target: (any WKScriptMessageHandler)?
    func userContentController(_ uc: WKUserContentController, didReceive message: WKScriptMessage) {
        target?.userContentController(uc, didReceive: message)
    }
}

/// Cold-open timings (shown in the report; also appended to `tmp/zoen-perf.log`).
enum MiniAppPerf {
    static func record(_ line: String) {
        print("[zoen perf]", line)
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("zoen-perf.log")
        let data = Data((ISO8601DateFormatter().string(from: .now) + " " + line + "\n").utf8)
        if let h = try? FileHandle(forWritingTo: url) {
            h.seekToEndOfFile(); h.write(data); try? h.close()
        } else {
            try? data.write(to: url)
        }
    }
}

// MARK: - Consent (layer 1: Zoen's sheet)

enum ConsentDecision { case once, always, deny }

struct ConsentRequest: Identifiable {
    let id = UUID()
    let capability: String
    let appName: String
    let spaceTitle: String
    let purpose: String
    let decide: (ConsentDecision) -> Void
}

/// What each capability means in plain words, and the pose Zo strikes asking for it.
enum CapabilityCopy {
    static func data(_ cap: String) -> String {
        switch cap {
        case "location.approximate": String(localized: "Your approximate location (about 3 km)")
        case "location": String(localized: "Your precise location")
        case "photos.pick": String(localized: "Only the photos you pick")
        case "camera.capture": String(localized: "One photo you take now")
        case "calendar.freebusy": String(localized: "When you’re busy (no event names)")
        case "calendar.events": String(localized: "Your event names and details")
        case "calendar.add": String(localized: "A new event, after you review it")
        case "contacts.pick": String(localized: "Only the contact you pick (name)")
        case "health.steps": String(localized: "Your daily step count")
        default:
            cap.hasPrefix("net:") ? String(localized: "Internet access to \(String(cap.dropFirst(4)))") : cap
        }
    }

    static func symbol(_ cap: String) -> String {
        switch cap {
        case "location.approximate", "location": "location.fill"
        case "photos.pick": "photo.on.rectangle"
        case "camera.capture": "camera.fill"
        case "calendar.freebusy", "calendar.events", "calendar.add": "calendar"
        case "contacts.pick": "person.crop.circle"
        case "health.steps": "figure.walk"
        default: "network"
        }
    }

    static func pose(_ cap: String) -> MascotPose {
        switch cap {
        case "location.approximate", "location": .map
        case "photos.pick", "camera.capture": .cheer
        case "calendar.freebusy", "calendar.events", "calendar.add": .walk
        case "contacts.pick": .phone
        case "health.steps": .run
        default: .shield
        }
    }

    /// Writes get their own native confirmation later; reads are just this sheet.
    static func isWrite(_ cap: String) -> Bool { cap == "calendar.add" }
}

/// The first layer: Zoen asks in its own voice, naming the mini-app, the data, the
/// purpose (from the manifest) and the scope. iOS asks the second time, only once.
struct ZoenConsentSheet: View {
    let request: ConsentRequest
    var onDone: () -> Void

    var body: some View {
        VStack(spacing: 16) {
            MascotView(pose: CapabilityCopy.pose(request.capability))
                .frame(width: 118, height: 118)
                .padding(.top, 14)
                .accessibilityHidden(true)
            Text("Let “\(request.appName)” use this?")
                .font(.title3.weight(.bold))
                .multilineTextAlignment(.center)
            VStack(alignment: .leading, spacing: 12) {
                row(CapabilityCopy.symbol(request.capability), CapabilityCopy.data(request.capability), bold: true)
                row("text.quote", request.purpose)
                row("person.2", String(localized: "Only this mini-app, in \(request.spaceTitle). Nothing goes to the group unless you share it."))
                if CapabilityCopy.isWrite(request.capability) {
                    row("checkmark.shield", String(localized: "You’ll see every change before it’s saved."))
                }
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.surface, in: .rect(cornerRadius: 22, style: .continuous))
            VStack(spacing: 10) {
                Button { decide(.always) } label: {
                    Text("Always for this app").font(.headline).frame(maxWidth: .infinity).frame(height: 50)
                }
                .buttonStyle(.glassProminent)
                .tint(Palette.action)
                Button { decide(.once) } label: {
                    Text("Allow once").font(.headline).frame(maxWidth: .infinity).frame(height: 50)
                }
                .buttonStyle(.glass)
                Button { decide(.deny) } label: {
                    Text("Don’t allow").font(.subheadline.weight(.semibold)).frame(maxWidth: .infinity).frame(height: 40)
                }
                .buttonStyle(.plain)
                .foregroundStyle(Palette.textSecondary)
            }
            Text("Change it any time in You › Mini-app access.")
                .font(.caption).foregroundStyle(Palette.textTertiary)
        }
        .padding(.horizontal, 22)
        .padding(.bottom, 10)
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
        .interactiveDismissDisabled()
    }

    private func row(_ symbol: String, _ text: String, bold: Bool = false) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Image(systemName: symbol).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.action).frame(width: 22)
            Text(text).font(bold ? .subheadline.weight(.semibold) : .subheadline)
                .foregroundStyle(bold ? Palette.textPrimary : Palette.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func decide(_ d: ConsentDecision) {
        d == .deny ? Haptics.dismiss() : Haptics.commit()
        request.decide(d)
        onDone()
    }
}

// MARK: - Photo vault (picked photos never reach shared state without "Share with group")

@MainActor
enum PhotoVault {
    /// token → (item, JPEG data URL). Tokens die with the app process.
    private static var store: [String: (item: String, dataUrl: String)] = [:]

    static func put(_ dataUrl: String, item: String) -> String {
        let token = "zoen-photo:" + UUID().uuidString.lowercased()
        store[token] = (item, dataUrl)
        return token
    }

    static func resolve(_ token: String, item: String) -> String? {
        guard let e = store[token], e.item == item else { return nil }
        return e.dataUrl
    }
}

// MARK: - Native capability performers (layer 2: iOS prompts and pickers)

struct NativeError: LocalizedError {
    let message: String
    var errorDescription: String? { message }
    init(_ m: String) { message = m }
}

@MainActor
enum NativeCapabilities {
    /// What this host can really do on this device (advertised to the app).
    static var available: [String] {
        #if os(iOS)
        var caps = ["photos.pick", "location", "location.approximate", "calendar.freebusy", "calendar.add", "contacts.pick"]
        if UIImagePickerController.isSourceTypeAvailable(.camera) { caps.append("camera.capture") }
        #else
        var caps = ["location", "location.approximate", "calendar.freebusy"]
        #endif
        if healthEnabled { caps.append("health.steps") }
        return caps
    }

    /// HealthKit is behind an entitlement we don't ship: off unless `-RodaHealthKit YES`, and
    /// even then it answers honestly that it isn't wired.
    static var healthEnabled: Bool { UserDefaults.standard.bool(forKey: "RodaHealthKit") }

    static func perform(_ cap: String, params: [String: Any], item: String) async throws -> Any {
        switch cap {
        case "location", "location.approximate":
            return try await location(approximate: cap == "location.approximate")
        case "calendar.freebusy":
            return try await freeBusy(params)
        case "calendar.add":
            return try await addEvent(params)
        case "health.steps":
            throw NativeError(String(localized: "Health isn’t connected in this build."))
        #if os(iOS)
        case "photos.pick":
            return try await pickPhotos(max: min(6, max(1, params["max"] as? Int ?? 4)), maxSide: CGFloat(min(1600, max(320, params["maxSide"] as? Int ?? 1024))), item: item)
        case "contacts.pick":
            return try await pickContact()
        case "camera.capture":
            return try await capturePhoto(item: item)
        #endif
        default:
            throw NativeError(String(localized: "This device can’t do \(cap)."))
        }
    }

    // Location: when-in-use only, one fix, never stored.
    static func location(approximate: Bool) async throws -> [String: Any] {
        #if os(iOS)
        let session = CLServiceSession(authorization: .whenInUse)
        defer { session.invalidate() }
        #else
        CLLocationManager().requestWhenInUseAuthorization()
        #endif
        let fix: CLLocation = try await withThrowingTaskGroup(of: CLLocation.self) { g in
            g.addTask {
                for try await u in CLLocationUpdate.liveUpdates(approximate ? .default : .otherNavigation) {
                    if u.authorizationDenied || u.authorizationDeniedGlobally { throw NativeError(String(localized: "Location is off for Zoen in Settings.")) }
                    if let l = u.location { return l }
                }
                throw NativeError(String(localized: "No location fix."))
            }
            g.addTask {
                try await Task.sleep(for: .seconds(12))
                throw NativeError(String(localized: "Location took too long."))
            }
            let first = try await g.next()!
            g.cancelAll()
            return first
        }
        if approximate {
            // ~2 km grid: enough for "how far is the trailhead", not enough to find your door.
            let snap = { (v: Double) in (v / 0.02).rounded() * 0.02 }
            return ["lat": snap(fix.coordinate.latitude), "lon": snap(fix.coordinate.longitude), "accuracyM": 3000, "approximate": true]
        }
        return ["lat": fix.coordinate.latitude, "lon": fix.coordinate.longitude, "accuracyM": fix.horizontalAccuracy, "approximate": false]
    }

    // Calendar read: busy blocks only, merged, no titles.
    static func freeBusy(_ p: [String: Any]) async throws -> [[String: Any]] {
        let store = EKEventStore()
        guard try await store.requestFullAccessToEvents() else { throw NativeError(String(localized: "Calendar access is off for Zoen in Settings.")) }
        let start = Date(timeIntervalSince1970: (p["startMs"] as? Double ?? Date.now.timeIntervalSince1970 * 1000) / 1000)
        let end = min(Date(timeIntervalSince1970: (p["endMs"] as? Double ?? 0) / 1000), start.addingTimeInterval(31 * 86_400))
        guard end > start else { return [] }
        let events = store.events(matching: store.predicateForEvents(withStart: start, end: end, calendars: nil))
            .filter { !$0.isAllDay && $0.availability != .free }
            .sorted { $0.startDate < $1.startDate }
        var blocks: [(Date, Date)] = []
        for e in events {
            if let last = blocks.last, e.startDate <= last.1 { blocks[blocks.count - 1].1 = max(last.1, e.endDate) } else { blocks.append((e.startDate, e.endDate)) }
        }
        return blocks.map { ["startMs": $0.0.timeIntervalSince1970 * 1000, "endMs": $0.1.timeIntervalSince1970 * 1000] }
    }

    // Calendar write: the system editor is the confirmation; nothing saves until you tap Add.
    static func addEvent(_ p: [String: Any]) async throws -> [String: Any] {
        #if os(iOS)
        let store = EKEventStore()
        let ev = EKEvent(eventStore: store)
        ev.title = String((p["title"] as? String ?? "").prefix(120))
        ev.startDate = Date(timeIntervalSince1970: (p["startMs"] as? Double ?? Date.now.timeIntervalSince1970 * 1000) / 1000)
        ev.endDate = Date(timeIntervalSince1970: (p["endMs"] as? Double ?? 0) / 1000)
        if ev.endDate <= ev.startDate { ev.endDate = ev.startDate.addingTimeInterval(3600) }
        ev.location = (p["location"] as? String).map { String($0.prefix(160)) }
        ev.notes = (p["notes"] as? String).map { String($0.prefix(1000)) }
        let saved = try await EventEditor.present(event: ev, store: store)
        return ["saved": saved]
        #else
        throw NativeError(String(localized: "Adding events from mini-apps comes to the Mac later."))
        #endif
    }

    #if os(iOS)
    static func pickPhotos(max: Int, maxSide: CGFloat, item: String) async throws -> [[String: Any]] {
        let images = try await PhotoPicker.present(limit: max)
        var out: [[String: Any]] = []
        for data in images {
            guard let img = UIImage(data: data) else { continue }
            guard let (url, w, h) = jpegDataURL(img, maxSide: maxSide) else { continue }
            out.append(["token": PhotoVault.put(url, item: item), "dataUrl": url, "width": w, "height": h, "locationStripped": true])
        }
        return out
    }

    static func capturePhoto(item: String) async throws -> [String: Any] {
        guard UIImagePickerController.isSourceTypeAvailable(.camera) else { throw NativeError(String(localized: "There’s no camera on this device.")) }
        let img = try await CameraCapture.present()
        guard let (url, w, h) = jpegDataURL(img, maxSide: 1024) else { throw NativeError(String(localized: "Couldn’t read the photo.")) }
        return ["token": PhotoVault.put(url, item: item), "dataUrl": url, "width": w, "height": h, "locationStripped": true]
    }

    static func pickContact() async throws -> [[String: Any]] {
        let names = try await ContactPicker.present()
        return names.map { n in ["name": n, "initials": n.split(separator: " ").prefix(2).compactMap(\.first).map(String.init).joined()] }
    }

    /// Re-encodes from pixels: EXIF (GPS included) doesn't survive. Shrinks until the data
    /// URL fits the core's 150 KB limit.
    static func jpegDataURL(_ img: UIImage, maxSide: CGFloat) -> (String, Int, Int)? {
        var side = maxSide
        for _ in 0..<5 {
            let scale = min(1, side / max(img.size.width, img.size.height))
            let size = CGSize(width: (img.size.width * scale).rounded(), height: (img.size.height * scale).rounded())
            let fmt = UIGraphicsImageRendererFormat(); fmt.scale = 1
            let small = UIGraphicsImageRenderer(size: size, format: fmt).image { _ in img.draw(in: CGRect(origin: .zero, size: size)) }
            if let d = small.jpegData(compressionQuality: 0.62) {
                let url = "data:image/jpeg;base64," + d.base64EncodedString()
                if url.count <= 150_000 { return (url, Int(size.width), Int(size.height)) }
            }
            side *= 0.75
        }
        return nil
    }

    static func topController() -> UIViewController? {
        let scene = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first { $0.activationState == .foregroundActive }
            ?? UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first
        var vc = scene?.keyWindow?.rootViewController ?? scene?.windows.first?.rootViewController
        while let next = vc?.presentedViewController, !next.isBeingDismissed { vc = next }
        return vc
    }
    #endif
}

#if os(iOS)
/// Collects picker results off the main thread.
private final class DataBox: @unchecked Sendable {
    private let lock = NSLock()
    private var items: [Data?]
    init(_ n: Int) { items = Array(repeating: nil, count: n) }
    func set(_ i: Int, _ d: Data?) { lock.withLock { items[i] = d } }
    var all: [Data] { lock.withLock { items.compactMap { $0 } } }
}

/// PHPicker: out of process, needs no library permission, returns only what you picked.
@MainActor
final class PhotoPicker: NSObject, PHPickerViewControllerDelegate {
    private var cont: CheckedContinuation<[Data], Error>?
    private static var live: PhotoPicker?

    static func present(limit: Int) async throws -> [Data] {
        guard let top = NativeCapabilities.topController() else { throw NativeError("No window") }
        var cfg = PHPickerConfiguration()
        cfg.filter = .images
        cfg.selectionLimit = limit
        cfg.preferredAssetRepresentationMode = .compatible
        let picker = PHPickerViewController(configuration: cfg)
        let me = PhotoPicker()
        live = me
        picker.delegate = me
        return try await withCheckedThrowingContinuation { c in
            me.cont = c
            top.present(picker, animated: true)
        }
    }

    func picker(_ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]) {
        picker.dismiss(animated: true)
        let box = DataBox(results.count)
        let group = DispatchGroup()
        for (i, r) in results.enumerated() {
            group.enter()
            _ = r.itemProvider.loadDataRepresentation(for: .image) { data, _ in
                box.set(i, data)
                group.leave()
            }
        }
        group.notify(queue: .main) {
            MainActor.assumeIsolated {
                self.cont?.resume(returning: box.all)
                self.cont = nil
                Self.live = nil
            }
        }
    }
}

@MainActor
final class ContactPicker: NSObject, @preconcurrency CNContactPickerDelegate {
    private var cont: CheckedContinuation<[String], Error>?
    private static var live: ContactPicker?

    static func present() async throws -> [String] {
        guard let top = NativeCapabilities.topController() else { throw NativeError("No window") }
        let picker = CNContactPickerViewController()
        picker.displayedPropertyKeys = []
        let me = ContactPicker()
        live = me
        picker.delegate = me
        return try await withCheckedThrowingContinuation { c in
            me.cont = c
            top.present(picker, animated: true)
        }
    }

    func contactPicker(_ picker: CNContactPickerViewController, didSelect contact: CNContact) {
        let name = [contact.givenName, contact.familyName].filter { !$0.isEmpty }.joined(separator: " ")
        cont?.resume(returning: name.isEmpty ? [] : [name]); cont = nil; Self.live = nil
    }

    func contactPickerDidCancel(_ picker: CNContactPickerViewController) {
        cont?.resume(returning: []); cont = nil; Self.live = nil
    }
}

@MainActor
final class EventEditor: NSObject, @preconcurrency EKEventEditViewDelegate {
    private var cont: CheckedContinuation<Bool, Error>?
    private static var live: EventEditor?

    static func present(event: EKEvent, store: EKEventStore) async throws -> Bool {
        guard let top = NativeCapabilities.topController() else { throw NativeError("No window") }
        let vc = EKEventEditViewController()
        vc.eventStore = store
        vc.event = event
        let me = EventEditor()
        live = me
        vc.editViewDelegate = me
        return try await withCheckedThrowingContinuation { c in
            me.cont = c
            top.present(vc, animated: true)
        }
    }

    func eventEditViewController(_ controller: EKEventEditViewController, didCompleteWith action: EKEventEditViewAction) {
        controller.dismiss(animated: true)
        cont?.resume(returning: action == .saved); cont = nil; Self.live = nil
    }
}

@MainActor
final class CameraCapture: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
    private var cont: CheckedContinuation<UIImage, Error>?
    private static var live: CameraCapture?

    static func present() async throws -> UIImage {
        guard let top = NativeCapabilities.topController() else { throw NativeError("No window") }
        let vc = UIImagePickerController()
        vc.sourceType = .camera
        let me = CameraCapture()
        live = me
        vc.delegate = me
        return try await withCheckedThrowingContinuation { c in
            me.cont = c
            top.present(vc, animated: true)
        }
    }

    func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
        picker.dismiss(animated: true)
        if let img = info[.originalImage] as? UIImage { cont?.resume(returning: img) } else { cont?.resume(throwing: NativeError("No photo")) }
        cont = nil; Self.live = nil
    }

    func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
        picker.dismiss(animated: true)
        cont?.resume(throwing: NativeError(String(localized: "You didn’t take a photo."))); cont = nil; Self.live = nil
    }
}
#endif
