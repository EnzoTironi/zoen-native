import XCTest

/// Reading a chat while friends keep writing: the chat never yanks you down. New messages
/// wait behind a "↓ N novas mensagens" capsule; tapping it takes you to the latest one.
/// At the end of the chat, new messages simply show up (no capsule).
final class ChatReadingJourneyTests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    @MainActor private func launch(fast: Bool) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-RodaDemo", "YES", "-RodaFreshStart", "YES", "-RodaResetDemo", "YES",
                               "-AppleLanguages", "(pt-BR)", "-RodaAppearance", "light",
                               "-RodaApprovalsExplained", "YES", "-RodaStory", "incoming",
                               "-RodaIncomingFast", fast ? "YES" : "NO"]
        app.launch()
        return app
    }

    @MainActor private func text(_ app: XCUIApplication, _ prefix: String) -> XCUIElement {
        app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", prefix)).firstMatch
    }

    @MainActor
    func testReadingHistoryKeepsYourPlaceAndPillJumpsToNew() {
        let app = launch(fast: false)
        let composer = app.textViews.firstMatch.exists ? app.textViews.firstMatch : app.textFields.firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 25), "the group chat opens")
        sleep(1)
        // Scroll back through the history, like reading an earlier part of the conversation.
        app.swipeDown(velocity: .slow)
        app.swipeDown(velocity: .slow)
        sleep(1)
        let anchor = app.staticTexts.allElementsBoundByIndex.first { $0.isHittable && $0.frame.minY > 220 && $0.label.count > 12 }
        XCTAssertNotNil(anchor, "a message is on screen while reading")
        let label = anchor!.label
        let before = anchor!.frame.minY

        let pill = app.buttons["new-messages-pill"]
        XCTAssertTrue(pill.waitForExistence(timeout: 20), "new messages wait behind a capsule")
        let two = NSPredicate(format: "label CONTAINS %@", "2 novas mensagens")
        expectation(for: two, evaluatedWith: pill)
        waitForExpectations(timeout: 6)
        let still = app.staticTexts[label]
        XCTAssertTrue(still.exists && still.isHittable, "the message you were reading stays on screen")
        XCTAssertEqual(still.frame.minY, before, accuracy: 6, "…in the same place (no jump)")
        XCTAssertFalse(text(app, "guarda um lugar").isHittable, "the new message hasn't pulled the chat down")

        pill.tap()
        XCTAssertTrue(pill.waitForNonExistence(timeout: 4), "the capsule goes away")
        sleep(1)
        XCTAssertTrue(text(app, "guarda um lugar").isHittable, "tapping it lands on the latest message")
    }

    @MainActor
    func testAtTheEndNewMessagesJustAppear() {
        let app = launch(fast: true)
        let newest = text(app, "guarda um lugar")
        XCTAssertTrue(newest.waitForExistence(timeout: 30), "the friend's message arrives")
        sleep(1)
        XCTAssertTrue(newest.isHittable, "at the end of the chat it comes into view")
        XCTAssertFalse(app.buttons["new-messages-pill"].exists, "no capsule when you're already at the end")
    }
}
