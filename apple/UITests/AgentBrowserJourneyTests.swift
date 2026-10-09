import XCTest

/// Zoen books the inn in its browser while you watch from the chat. The site asks for a
/// password, Zoen stops and asks; you take over, type it (Zoen is stopped and can't see it),
/// hand it back with "Pronto", and Zoen finishes. Back (the chevron) returns to the chat.
final class AgentBrowserJourneyTests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    @MainActor private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-RodaDemo", "YES", "-RodaFreshStart", "YES", "-RodaResetDemo", "YES",
                               "-AppleLanguages", "(pt-BR)", "-RodaAppearance", "light",
                               "-RodaApprovalsExplained", "YES", "-RodaOpen", "paraty", "-RodaAgentBrowser", "YES"]
        app.launch()
        return app
    }

    @MainActor private func shot(_ app: XCUIApplication, _ name: String) {
        let a = XCTAttachment(screenshot: app.screenshot()); a.name = name; a.lifetime = .keepAlways; add(a)
    }

    @MainActor
    func testWatchTakeOverSignInAndHandBack() {
        let app = launch()
        let any = app.descendants(matching: .any)
        let title = any["browser-title"]
        XCTAssertTrue(title.waitForExistence(timeout: 25), "Paraty shows Zoen's browser card")
        // On a slow machine it may already be at the sign-in by now; either way it's live.
        XCTAssertTrue(title.label.contains("usando o navegador") || title.label.contains("precisa de você"), "while it works: \(title.label)")
        XCTAssertTrue(app.staticTexts["Ao vivo"].exists, "the screen is live")
        shot(app, "browser-live")

        // Watch it full screen, then Back returns to the chat.
        any["browser-live"].tap()
        XCTAssertTrue(any["browser-screen"].waitForExistence(timeout: 5), "the agent's screen fills the phone")
        app.buttons["browser-close"].tap()
        XCTAssertTrue(any["browser-screen"].waitForNonExistence(timeout: 4), "Back pops to the chat")
        XCTAssertTrue(title.exists)

        // The site asks for a password: Zoen stops and asks.
        let takeover = app.buttons["browser-takeover"]
        XCTAssertTrue(takeover.waitForExistence(timeout: 15), "the takeover card appears")
        XCTAssertTrue(title.label.contains("precisa de você"), "\(title.label)")
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label CONTAINS 'não digita senhas'")).firstMatch.exists,
                      "the card says why")
        shot(app, "browser-needs-you")

        // "Agora não" leaves it waiting; you can still take over.
        app.buttons["browser-not-now"].tap()
        XCTAssertTrue(app.buttons["browser-not-now"].waitForNonExistence(timeout: 3))
        XCTAssertTrue(title.label.contains("esperando"), "\(title.label)")
        XCTAssertTrue(takeover.exists)

        takeover.tap()
        let field = app.secureTextFields["browser-password"]
        XCTAssertTrue(field.waitForExistence(timeout: 5), "taking over opens the screen with a password field")
        XCTAssertTrue(any["browser-stopped"].exists, "it says Zoen is stopped and can't see it")
        let done = app.buttons["browser-done"]
        XCTAssertFalse(done.isEnabled, "nothing to hand back yet")
        if !field.hasFocus { field.tap() }
        app.typeText("praia2026")
        XCTAssertTrue(done.isEnabled, "typed: can hand back")
        shot(app, "browser-driving")
        done.tap()

        let back = app.buttons["browser-back-to-chat"]
        XCTAssertTrue(back.waitForExistence(timeout: 5), "Zoen finished")
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label CONTAINS 'confirmou a reserva'")).firstMatch.exists)
        back.tap()
        XCTAssertTrue(any["browser-screen"].waitForNonExistence(timeout: 4))
        XCTAssertTrue(title.waitForExistence(timeout: 3))
        XCTAssertTrue(title.label.contains("Reserva confirmada"), "the card tells what happened: \(title.label)")
        XCTAssertTrue(any["browser-finished"].exists)
        XCTAssertFalse(app.staticTexts["praia2026"].exists, "the password shows nowhere")
        shot(app, "browser-finished")
    }
}
