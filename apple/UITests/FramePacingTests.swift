import XCTest

/// Drives the approval card like a finger (slow pulls that spring back, a slow decide, a
/// flick) with the debug frame-pacing probe on. The probe writes one `ZPACE` line per drag
/// to the app's tmp/zpace.log; the runner script reads it back from the app container.
final class FramePacingTests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    @MainActor
    func testCardDragFramePacing() {
        let app = XCUIApplication()
        app.launchArguments = ["-RodaDemo", "YES", "-RodaShowcase", "YES", "-AppleLanguages", "(pt-BR)",
                               "-RodaAppearance", "light", "-RodaApprovalsExplained", "YES",
                               "-RodaFreshStart", "YES", "-RodaResetDemo", "YES",
                               "-RodaOpen", "aprovacoes", "-RodaFramePacing", "YES"]
        app.launch()
        let card = app.descendants(matching: .any)["approval-card"].firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 20), "the stack opens on a card")
        sleep(5) // the app measures an idle baseline first
        func pull(_ dx: CGFloat, _ dy: CGFloat, speed: CGFloat, hold: TimeInterval) {
            let g = card.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.35))
            g.press(forDuration: 0.12, thenDragTo: g.withOffset(CGVector(dx: dx, dy: dy)),
                    withVelocity: XCUIGestureVelocity(speed), thenHoldForDuration: hold)
            sleep(1)
        }
        let label = card.label
        // Hesitant pulls that spring back: the bulk of the dragging people do.
        pull(90, 12, speed: 180, hold: 0.4)
        pull(-90, -8, speed: 180, hold: 0.4)
        pull(20, -110, speed: 160, hold: 0.4)
        pull(-20, 110, speed: 160, hold: 0.4)
        XCTAssertEqual(card.label, label, "short pulls don't decide")
        // A slow decide, then a flick.
        pull(240, 20, speed: 220, hold: 0.2)
        pull(-140, -20, speed: 2400, hold: 0)
        sleep(1)
    }
}
