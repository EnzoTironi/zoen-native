import XCTest

/// A Release build against the staging relay, onboarded through the real UI (Release builds
/// have no shortcut flags): sign up, wait for a person's first message, answer it.
/// `scripts/journey-staging.sh` builds, installs fresh and plays the other person from a
/// terminal on another machine.
final class StagingJourneyTests: XCTestCase {
    @MainActor
    func testSignUpOnStagingAndAnswerAPerson() throws {
        continueAfterFailure = false
        let env = ProcessInfo.processInfo.environment
        guard let me = env["ZOEN_ME"], let message = env["ZOEN_MESSAGE"], let reply = env["ZOEN_REPLY"] else {
            throw XCTSkip("run by scripts/journey-staging.sh")
        }
        let app = XCUIApplication()
        app.launchArguments = ["-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        app.launch()

        tap(app.buttons["Hi, Zoen!"], within: 20)
        let name = app.textFields["Your name"]
        XCTAssertTrue(name.waitForExistence(timeout: 10))
        name.tap()
        name.typeText("Ana")
        let handle = app.textFields["handle"]
        handle.tap()
        handle.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 30) + me)
        tap(app.buttons["Continue"], within: 5)
        keep(app, "staging: signed up")

        tap(app.buttons.matching(NSPredicate(format: "label CONTAINS 'Trips'")).firstMatch, within: 30)
        tap(app.buttons["Continue"], within: 5)
        tap(app.buttons["Looks good"], within: 60)
        tap(app.buttons["Skip"], within: 10)
        let open = app.buttons["Open Zoen"]
        if open.waitForExistence(timeout: 5) { open.tap() }

        // Onboarding ends inside the Zoen chat; the person's chat is in the list behind it.
        let back = app.buttons["Back"]
        if back.waitForExistence(timeout: 10) { back.tap() }
        let chat = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", message)).firstMatch
        XCTAssertTrue(chat.waitForExistence(timeout: 180), "the person's message arrives through relay.tryzoen.com")
        XCTAssertTrue(chat.isHittable, "the chat list is on screen")
        keep(app, "staging: message arrived")
        chat.tap()
        let composer = app.descendants(matching: .any)["composer"]
        XCTAssertTrue(composer.waitForExistence(timeout: 10))
        composer.tap()
        composer.typeText(reply)
        app.buttons.matching(NSPredicate(format: "identifier ==[c] 'send' AND label == 'Send'")).firstMatch.tap()
        XCTAssertTrue(app.staticTexts[reply].waitForExistence(timeout: 10))
        keep(app, "staging: answered")
    }

    @MainActor
    private func tap(_ element: XCUIElement, within seconds: Double) {
        XCTAssertTrue(element.waitForExistence(timeout: seconds), "\(element) never appeared")
        let deadline = Date().addingTimeInterval(seconds)
        while !element.isEnabled && Date() < deadline { RunLoop.current.run(until: Date().addingTimeInterval(0.25)) }
        element.tap()
    }

    @MainActor
    private func keep(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }
}
