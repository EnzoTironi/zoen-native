import XCTest

/// Reading a chat while friends keep writing: the chat never yanks you down. New messages
/// wait behind a "↓ N novas mensagens" capsule; tapping it takes you to the latest one.
/// At the end of the chat, new messages simply show up (no capsule).
final class ChatReadingJourneyTests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    @MainActor private func launch(fast: Bool, story: String = "incoming") -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-RodaDemo", "YES", "-RodaFreshStart", "YES", "-RodaResetDemo", "YES",
                               "-AppleLanguages", "(pt-BR)", "-RodaAppearance", "light",
                               "-RodaApprovalsExplained", "YES", "-RodaStory", story,
                               "-RodaIncomingFast", fast ? "YES" : "NO",
                               "-RodaIncomingManual", !fast && story == "incoming" ? "YES" : "NO"]
        app.launch()
        return app
    }

    @MainActor private func text(_ app: XCUIApplication, _ prefix: String) -> XCUIElement {
        app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", prefix)).firstMatch
    }

    @MainActor private func capture(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    @MainActor
    func testReadingHistoryKeepsYourPlaceAndPillJumpsToNew() {
        let app = launch(fast: false)
        let deadline = Date().addingTimeInterval(25)
        while !(app.textViews.firstMatch.exists || app.textFields.firstMatch.exists), Date() < deadline { usleep(300_000) }
        XCTAssertTrue(app.textViews.firstMatch.exists || app.textFields.firstMatch.exists, "the group chat opens")
        sleep(1)
        // Scroll back through the history, like reading an earlier part of the conversation.
        let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.35))
        let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.78))
        start.press(forDuration: 0.05, thenDragTo: end)
        start.press(forDuration: 0.05, thenDragTo: end)
        sleep(1)
        let anchor = app.staticTexts.allElementsBoundByIndex.first { $0.isHittable && $0.frame.minY > 220 && $0.label.count > 12 }
        XCTAssertNotNil(anchor, "a message is on screen while reading")
        let label = anchor!.label
        let before = anchor!.frame.minY
        XCTAssertFalse(text(app, "guarda um lugar").exists, "incoming messages are held until after scrolling")
        capture("Reading history before new messages")
        app.buttons["demo-incoming-trigger"].tap()

        let pill = app.buttons["new-messages-pill"]
        XCTAssertTrue(pill.waitForExistence(timeout: 20), "new messages wait behind a capsule")
        let two = NSPredicate(format: "label CONTAINS %@", "2 novas mensagens")
        expectation(for: two, evaluatedWith: pill)
        waitForExpectations(timeout: 6)
        let still = app.staticTexts[label]
        XCTAssertTrue(still.exists && still.isHittable, "the message you were reading stays on screen")
        XCTAssertEqual(still.frame.minY, before, accuracy: 6, "…in the same place (no jump)")
        XCTAssertFalse(text(app, "guarda um lugar").isHittable, "the new message hasn't pulled the chat down")
        capture("Reading position held with two new messages")

        pill.tap()
        XCTAssertTrue(pill.waitForNonExistence(timeout: 4), "the capsule goes away")
        sleep(1)
        XCTAssertTrue(text(app, "guarda um lugar").isHittable, "tapping it lands on the latest message")
        capture("Capsule jumps to the latest message")
    }

    @MainActor
    func testOpeningUnreadHistoryStartsAtTheFirstUnreadMessage() {
        let app = launch(fast: false, story: "unread")
        let divider = app.descendants(matching: .any).matching(identifier: "unread-divider").firstMatch
        XCTAssertTrue(divider.waitForExistence(timeout: 15), "the unread boundary appears")
        XCTAssertTrue(divider.isHittable, "opening the chat brings its unread boundary into view")
        XCTAssertTrue(text(app, "Mensagem não lida 1:").isHittable, "the first unread message is visible")
        XCTAssertFalse(text(app, "Mensagem não lida 12:").isHittable, "opening unread history does not skip to its end")
        capture("Chat opens at the first unread message")
    }

    @MainActor
    func testMixedHistoryStartsAtTheFirstUnreadIncomingMessage() {
        let app = launch(fast: false, story: "unread-mixed")
        let divider = app.descendants(matching: .any).matching(identifier: "unread-divider").firstMatch
        XCTAssertTrue(divider.waitForExistence(timeout: 15), "the mixed history has an unread boundary")
        XCTAssertTrue(divider.isHittable, "opening mixed history brings its unread boundary into view")
        XCTAssertTrue(text(app, "Mensagem mista não lida 1:").isHittable,
                      "own messages, background changes and thread replies do not shift the first unread message")
        XCTAssertFalse(text(app, "Mensagem mista não lida 12:").isHittable,
                       "opening mixed history does not skip to its end")
        capture("Mixed chat opens at the first unread incoming message")
    }

    @MainActor
    func testOpeningAChatWithNoReadHistoryStartsAtTheFirstMessage() {
        let app = launch(fast: false, story: "unread-all")
        let divider = app.descendants(matching: .any).matching(identifier: "unread-divider").firstMatch
        XCTAssertTrue(divider.waitForExistence(timeout: 15), "an entirely unread chat has an unread boundary")
        XCTAssertTrue(divider.isHittable, "opening a chat for the first time brings the boundary into view")
        XCTAssertTrue(text(app, "Mensagem da primeira leitura 1:").isHittable, "its first message is visible")
        XCTAssertFalse(text(app, "Mensagem da primeira leitura 12:").isHittable,
                       "a chat without a read marker does not open at its newest message")
        capture("Entirely unread chat opens at the first message")
    }

    @MainActor
    func testAtTheEndNewMessagesJustAppear() {
        let app = launch(fast: true)
        let newest = text(app, "guarda um lugar")
        XCTAssertTrue(newest.waitForExistence(timeout: 30), "the friend's message arrives")
        sleep(1)
        XCTAssertTrue(newest.isHittable, "at the end of the chat it comes into view")
        XCTAssertFalse(app.buttons["new-messages-pill"].exists, "no capsule when you're already at the end")
        capture("New messages visible at the end of the chat")
    }
}
