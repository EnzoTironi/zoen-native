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
}
