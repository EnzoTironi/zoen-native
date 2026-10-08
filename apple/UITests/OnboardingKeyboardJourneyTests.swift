import XCTest

/// A person signing up types their name, then picks their @ with the keyboard up. The
/// title, the field they're typing in and Continue all stay on screen, above the keyboard,
/// with nothing overlapping. No tech talk on the step.
final class OnboardingKeyboardJourneyTests: XCTestCase {
    override func setUp() { continueAfterFailure = true }

    @MainActor
    func testPickingYourHandleFitsAboveTheKeyboard() {
        let app = XCUIApplication()
        app.launchArguments = ["-RodaOnboarding", "YES", "-RodaOnboardingStep", "profile",
                               "-AppleLanguages", "(pt-BR)"]
        app.launch()

        let name = app.textFields.element(boundBy: 0)
        XCTAssertTrue(name.waitForExistence(timeout: 20), "the name field is there")
        name.tap()
        name.typeText("Enzo Tironi\n")          // Next on the keyboard moves on to the @
        let handle = app.textFields.element(boundBy: 1)
        let keyboard = app.keyboards.firstMatch
        XCTAssertTrue(keyboard.waitForExistence(timeout: 5), "the keyboard is up")
        XCTAssertTrue((handle.value(forKey: "hasKeyboardFocus") as? Bool) ?? false, "typing goes into the @")
        sleep(1)   // let the layout settle with the keyboard
        keep(app, "onboarding-handle-keyboard")

        let kb = keyboard.frame
        let title = app.staticTexts.matching(NSPredicate(format: "label CONTAINS[c] 'amigos' OR label CONTAINS[c] 'friends'")).firstMatch
        XCTAssertTrue(title.exists, "the title is on screen")
        let next = app.buttons["Continuar"]
        XCTAssertTrue(next.exists, "Continue is on screen")
        let window = app.windows.firstMatch.frame
        XCTAssertGreaterThanOrEqual(title.frame.minY, window.minY + 44, "the title isn't cut off at the top")
        XCTAssertLessThanOrEqual(title.frame.maxY, name.frame.minY, "the title sits above the fields")
        XCTAssertLessThanOrEqual(handle.frame.maxY, next.frame.minY, "the @ field sits above Continue")
        XCTAssertLessThanOrEqual(next.frame.maxY, kb.minY + 1, "Continue sits above the keyboard")
        XCTAssertGreaterThan(next.frame.minY, kb.minY - 120, "Continue rides just above the keyboard")
        XCTAssertTrue(next.isHittable && handle.isHittable)

        let jargon = app.staticTexts.matching(NSPredicate(format: "label CONTAINS[c] 'Keychain' OR label CONTAINS[c] 'chaves' OR label CONTAINS[c] ' keys '"))
        XCTAssertEqual(jargon.count, 0, "no tech talk on the step")
    }

    @MainActor
    private func keep(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }
}
