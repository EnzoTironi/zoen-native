import XCTest
import UIKit
@testable import ZoenCore

// AUTHORED, NOT_RUN. Requires a hosted iOS XCTest target with a foreground scene.
// These tests qualify pending props, actual UIView window attachment and admission
// refusal/fences. Actual Fabric tag lookup and inputView installation remain an
// integrated native journey; the deliberately unknown tag must fail closed here.
@MainActor
final class KeyboardPanelAdmissionTests: XCTestCase {
  private final class ProbeHost: ZoenKeyboardPanelHost {
    var closed: [[String: Any]] = []
    override func emitClosedEvent(_ value: [String: Any]) { closed.append(value) }
  }
  private func binding(token: String, active: Bool = true, closing: Bool = false) throws -> String {
    let value: [String: Any] = ["token": token, "context": "synthetic-composer",
      "tag": 999_999_999, "label": "Synthetic composer", "active": active,
      "closing": closing, "returnToKeyboard": false]
    let data = try JSONSerialization.data(withJSONObject: value)
    return try XCTUnwrap(String(data: data, encoding: .utf8))
  }
  private func mountedWindow() throws -> UIWindow {
    let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
      .first { $0.activationState == .foregroundActive })
    let window = UIWindow(windowScene: scene)
    window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
    window.rootViewController = UIViewController()
    return window
  }
  private func remove(_ host: UIView, from window: UIWindow) {
    host.removeFromSuperview()
    window.isHidden = true
    window.rootViewController = nil
  }
  func testActivePropsBeforeWindowRemainPendingAndMountRetriesFullValidation() throws {
    let host = ProbeHost(), token = UUID().uuidString
    host.binding = try binding(token: token)
    host.model = "{}"
    XCTAssertNil(host.window)
    host.applyProps()
    host.applyProps()
    XCTAssertTrue(host.closed.isEmpty, "off-window props must not terminally retire the presentation")

    let window = try mountedWindow()
    defer { remove(host, from: window) }
    try XCTUnwrap(window.rootViewController).view.addSubview(host)
    XCTAssertTrue(host.window === window)
    // didMoveToWindow retried admission. The synthetic tag intentionally has no
    // editor: reaching its strict validation must reject exactly once now.
    XCTAssertEqual(host.closed.count, 1)
    XCTAssertEqual(host.closed.first?["token"] as? String, token)
    XCTAssertEqual(host.closed.first?["reason"] as? String, "unavailable")
    host.applyProps()
    XCTAssertEqual(host.closed.count, 1, "mount must not revive a truly rejected token")
  }
  func testCancelBeforeMountStillRetiresPendingTokenAndDoesNotReopenOnAttach() throws {
    let host = ProbeHost(), token = UUID().uuidString
    host.binding = try binding(token: token)
    host.applyProps()
    XCTAssertTrue(host.closed.isEmpty)
    host.binding = try binding(token: token, active: false, closing: true)
    host.applyProps()
    XCTAssertEqual(host.closed.count, 1)
    XCTAssertEqual(host.closed.first?["reason"] as? String, "closed")

    let window = try mountedWindow()
    defer { remove(host, from: window) }
    try XCTUnwrap(window.rootViewController).view.addSubview(host)
    host.binding = try binding(token: token)
    host.applyProps()
    XCTAssertEqual(host.closed.count, 1)
    XCTAssertEqual(host.closed.first?["reason"] as? String, "closed")
  }
  func testGlobalRetirementFencesPendingHostEvenAfterAdmissionsResume() async throws {
    let host = ProbeHost(), token = UUID().uuidString
    host.binding = try binding(token: token)
    host.applyProps()
    XCTAssertTrue(host.closed.isEmpty)
    let ticket = await ZoenKeyboardPanelHost.retireAll()
    defer { _ = ZoenKeyboardPanelHost.resumeAdmissions(expected: ticket) }

    let window = try mountedWindow()
    defer { remove(host, from: window) }
    try XCTUnwrap(window.rootViewController).view.addSubview(host)
    XCTAssertTrue(host.closed.isEmpty, "retired pending host cannot reach editor admission")
    XCTAssertTrue(ZoenKeyboardPanelHost.resumeAdmissions(expected: ticket))
    host.binding = try binding(token: UUID().uuidString)
    host.applyProps()
    XCTAssertTrue(host.closed.isEmpty, "resuming the session cannot revive an old retired host")

    let fresh = ProbeHost()
    defer { fresh.removeFromSuperview() }
    fresh.binding = try binding(token: UUID().uuidString)
    try XCTUnwrap(window.rootViewController).view.addSubview(fresh)
    XCTAssertEqual(fresh.closed.first?["reason"] as? String, "unavailable",
      "a fresh host reaches strict tag validation after admissions resume")
  }

  /// This double holds the real host's camera-stop boundary open. It does not
  /// replace the production File renderer tested in RecentAttachmentContentTests.
  private final class HeldContent: ZoenAttachmentOverlayContent {
    let view = UIView(), attachmentScrollView = UIScrollView()
    let attachmentHasKeyboardFocus = false
    let attachmentAnimatedViews: [UIView] = []
    var attachmentDidBeginEditing: (() -> Void)?
    var attachmentPresentationChanged: (() -> Void)?
    var finishStop: (@MainActor () -> Void)?
    var send: (String, [String: Any]) -> Void = { _, _ in }
    private(set) var retirements = 0
    func update(model: [String: Any], appearance: ZoenKeyboardPanelAppearance) throws {}
    func updatePresentation(expansion: CGFloat, keyboardHeight: CGFloat, safeArea: UIEdgeInsets) {}
    func retire() { retirements += 1 }
    func retireForDismissal(completion: @escaping @MainActor () -> Void) {
      retire(); finishStop = completion
    }
  }
  private final class DismissalHost: ZoenKeyboardPanelHost {
    let held = HeldContent()
    var closed: [[String: Any]] = [], actions: [[String: Any]] = []
    var didClose: (() -> Void)?
    override var requiresMountedEditor: Bool { false }
    override func makeContent(send: @escaping (String, [String: Any]) -> Void) -> any ZoenKeyboardPanelContent {
      held.send = send; return held
    }
    override func emitActionEvent(_ value: [String: Any]) { actions.append(value) }
    override func emitClosedEvent(_ value: [String: Any]) { closed.append(value); didClose?() }
  }
  func testSensitiveRetirementOvertakesOrdinaryCloseBeforeCameraDrainWithoutRestoringAuthority() async throws {
    let window = try mountedWindow(), host = DismissalHost(), token = UUID().uuidString
    let request: [String: Any] = ["token": token, "context": "synthetic-dismissal", "active": true, "standalone": true]
    func json(_ value: [String: Any]) throws -> String {
      String(decoding: try JSONSerialization.data(withJSONObject: value), as: UTF8.self)
    }
    host.binding = try json(request); host.model = "{\"revision\":\"admitted\"}"
    window.isHidden = false
    defer { remove(host, from: window) }
    try XCTUnwrap(window.rootViewController).view.addSubview(host)
    XCTAssertNotNil(host.held.view.window); XCTAssertTrue(host.closed.isEmpty)
    host.held.send("caption", ["text": "before", "revision": "admitted"])
    XCTAssertEqual(host.actions.count, 1)
    var closedRequest = request; closedRequest["active"] = false
    host.binding = try json(closedRequest); host.applyProps()
    XCTAssertEqual(host.held.retirements, 1)
    XCTAssertNotNil(host.held.finishStop)
    XCTAssertFalse(host.held.view.isHidden, "ordinary close retains original pixels through the held stop")
    NotificationCenter.default.post(name: UIApplication.willResignActiveNotification, object: nil)
    XCTAssertTrue(host.held.view.isHidden, "sensitive retirement must conceal even an already retiring owner")
    XCTAssertTrue(host.closed.isEmpty, "closed cannot precede stop and actual removal")
    host.binding = try json(request); host.applyProps()
    host.held.send("caption", ["text": "after", "revision": "admitted"])
    XCTAssertEqual(host.actions.count, 1, "late active props and retained callbacks cannot revive the owner")
    let removed = expectation(description: "actual overlay removal")
    host.didClose = { removed.fulfill() }
    let finish = try XCTUnwrap(host.held.finishStop); host.held.finishStop = nil; finish()
    await fulfillment(of: [removed], timeout: 2)
    XCTAssertEqual(host.closed.count, 1); XCTAssertNil(host.held.view.superview)
    XCTAssertEqual(host.held.retirements, 1)
  }
}
