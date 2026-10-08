import XCTest

/// Long-hold the + → record → release → land in Zoen chat with a voice note.
final class PlusVoiceZoenTests: XCTestCase {
    @MainActor
    func testLongHoldPlusSendsVoiceToZoen() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = [
            "-RodaFreshStart", "YES", "-RodaDemo", "YES", "-RodaResetDemo", "YES",
            "-RodaVoiceDemo", "plus",
            "-AppleLanguages", "(en)",
        ]
        app.launch()

        let plus = app.buttons["Create and more"]
        XCTAssertTrue(plus.waitForExistence(timeout: 12), "radial + visible on home")
        // Press and hold centre to arm voice (fan opens then collapses into mic).
        plus.press(forDuration: 1.2)

        // Recording UI may flash briefly before release delivers + navigates.
        let recording = app.descendants(matching: .any)["Recording for Zoen"]
        _ = recording.waitForExistence(timeout: 3)

        // Release is implied after press(forDuration) returns — deliver + navigate.
        let zoen = app.staticTexts["Zoen"].firstMatch
        let back = app.buttons["zoenBack"]
        XCTAssertTrue(
            zoen.waitForExistence(timeout: 12) || back.waitForExistence(timeout: 8),
            "navigated to Zoen chat after voice send"
        )

        // Voice bubble landed in Zoen's thread (send-flight land).
        let voice = app.descendants(matching: .any)["voice-message"]
        XCTAssertTrue(
            voice.waitForExistence(timeout: 10),
            "voice message visible in Zoen chat"
        )
    }
}
