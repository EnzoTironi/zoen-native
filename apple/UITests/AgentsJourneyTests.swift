import XCTest

/// Seus agentes as an owner uses it: the list, an agent's profile, talking to it and
/// coming back one screen at a time. Shots in light and dark go to the PR.
final class AgentsJourneyTests: XCTestCase {
    private func launch(dark: Bool) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = [
            "-RodaFreshStart", "YES", "-RodaDemo", "YES", "-RodaResetDemo", "YES",
            "-RodaTab", "agents", "-AppleLanguages", "(pt-BR)", "-AppleLocale", "pt_BR",
            "-RodaAppearance", dark ? "dark" : "light",
        ]
        app.launch()
        return app
    }

    private func shot(_ app: XCUIApplication, _ name: String) {
        let a = XCTAttachment(screenshot: app.screenshot())
        a.name = name
        a.lifetime = .keepAlways
        add(a)
    }

    @MainActor
    func testListProfileTalkAndBackPopsOne() throws {
        continueAfterFailure = false
        for dark in [false, true] {
            let app = launch(dark: dark)
            let mode = dark ? "dark" : "light"
            let mine = app.descendants(matching: .any)["agents.mine"]
            XCTAssertTrue(mine.waitForExistence(timeout: 15), "Seus agentes lists the agents you own")
            XCTAssertTrue(app.staticTexts["Seus"].exists, "plain Portuguese section title")
            shot(app, "agents-list-\(mode)")

            let row = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'agentRow.'")).firstMatch
            XCTAssertTrue(row.waitForExistence(timeout: 5), "an agent row")
            row.tap()
            let talk = app.buttons["agent.chat"]
            XCTAssertTrue(talk.waitForExistence(timeout: 5), "the profile offers Conversar")
            XCTAssertTrue(app.staticTexts["Conversar"].exists)
            shot(app, "agent-profile-\(mode)")

            talk.tap()
            XCTAssertTrue(app.descendants(matching: .any)["composer"].waitForExistence(timeout: 8), "opens the chat")
            // Back pops exactly one screen: chat → profile.
            let back = app.buttons["zoenBack"].exists ? app.buttons["zoenBack"] : app.navigationBars.buttons.firstMatch
            back.tap()
            XCTAssertTrue(talk.waitForExistence(timeout: 5), "back on the agent's profile, not the list")
            app.terminate()
        }
    }
}
