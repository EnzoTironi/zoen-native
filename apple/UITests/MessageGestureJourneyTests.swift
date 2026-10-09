import XCTest

/// Drag a message like a finger would: LEFT → inline reply (the composer shows who you are
/// answering, the sent message carries the quote); RIGHT → reply in its thread (the thread
/// opens, the answer lands there, and the chat shows "1 resposta" under the message).
/// A drag that starts in the left edge zone belongs to the system back swipe: no thread.
final class MessageGestureJourneyTests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    @MainActor private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-RodaDemo", "YES", "-RodaFreshStart", "YES", "-RodaResetDemo", "YES",
                               "-AppleLanguages", "(pt-BR)", "-RodaAppearance", "light",
                               "-RodaApprovalsExplained", "YES", "-RodaOpen", "paraty"]
        app.launch()
        return app
    }

    @MainActor private func text(_ app: XCUIApplication, _ prefix: String) -> XCUIElement {
        app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", prefix)).firstMatch
    }

    @MainActor private func drag(_ el: XCUIElement, dx: CGFloat, fromX: CGFloat = 0.5) {
        let from = el.coordinate(withNormalizedOffset: CGVector(dx: fromX, dy: 0.5))
        from.press(forDuration: 0.05, thenDragTo: from.withOffset(CGVector(dx: dx, dy: 4)),
                   withVelocity: XCUIGestureVelocity(380), thenHoldForDuration: 0.25)
    }

    @MainActor private func tapSend(_ app: XCUIApplication) {
        let sends = app.buttons.matching(identifier: "send").allElementsBoundByIndex.filter { $0.isHittable }
        XCTAssertFalse(sends.isEmpty, "a send button is reachable")
        sends.last?.tap()
    }

    @MainActor
    func testDragLeftRepliesInlineAndRightRepliesInThread() {
        let app = launch()
        let close = app.buttons["thread-close"]

        // → Thread reply on the Organizer's packing-list message.
        let list = text(app, "Criei a lista de malas")
        XCTAssertTrue(list.waitForExistence(timeout: 20), "Paraty shows the Organizer's message")
        sleep(2)
        drag(list, dx: 160)
        XCTAssertTrue(close.waitForExistence(timeout: 4), "a right drag opens the message's thread")
        XCTAssertTrue(app.descendants(matching: .any)["thread-count"].exists, "the thread shows its root and count")
        sleep(1)
        app.typeText("Valeu! Vou olhar")
        tapSend(app)
        XCTAssertTrue(text(app, "Valeu! Vou olhar").waitForExistence(timeout: 5), "the answer lands in the thread")
        sleep(1)
        close.tap()
        XCTAssertTrue(close.waitForNonExistence(timeout: 4))
        let chip = app.buttons["thread-chip"]
        XCTAssertTrue(chip.waitForExistence(timeout: 4), "the chat shows the thread under the message")
        XCTAssertTrue(chip.label.contains("1 resposta"), "…with its count: \(chip.label)")
        XCTAssertFalse(text(app, "Valeu! Vou olhar").exists, "thread replies stay out of the main chat")
        chip.tap()
        XCTAssertTrue(close.waitForExistence(timeout: 4), "tapping the count reopens the thread")
        sleep(1)
        close.tap()
        XCTAssertTrue(close.waitForNonExistence(timeout: 4))
        sleep(1)

        // ← Inline reply to Marina (her latest message, just above the composer).
        let marina = text(app, "@Enzo já aprovou")
        XCTAssertTrue(marina.waitForExistence(timeout: 5), "Marina's latest message")
        drag(marina, dx: -150)
        let bar = app.descendants(matching: .any)["reply-bar"]
        XCTAssertTrue(bar.waitForExistence(timeout: 4), "a left drag opens the reply bar above the composer")
        XCTAssertTrue(app.staticTexts["Respondendo a Marina"].exists, "it says who you're answering")
        // × drops it, a second drag brings it back.
        app.buttons["reply-cancel"].tap()
        XCTAssertTrue(bar.waitForNonExistence(timeout: 3), "× cancels the reply")
        sleep(1)
        drag(marina, dx: -150)
        XCTAssertTrue(bar.waitForExistence(timeout: 4))
        let composer = app.textFields["composer"].firstMatch
        composer.tap()
        composer.typeText("Aprovei agora!")
        tapSend(app)
        XCTAssertTrue(bar.waitForNonExistence(timeout: 4), "sending clears the reply bar")
        XCTAssertTrue(text(app, "Aprovei agora!").waitForExistence(timeout: 5), "the reply is in the chat")
        let quote = app.descendants(matching: .any).matching(identifier: "reply-quote").firstMatch
        XCTAssertTrue(quote.waitForExistence(timeout: 3), "the reply carries Marina's quote")
        XCTAssertTrue(quote.label.contains("Marina"), "the quote names Marina: \(quote.label)")
        sleep(1)
        sleep(1)

        // A pull that starts at the very left edge is the back swipe's, not a reply or thread.
        let sent = text(app, "Aprovei agora!")
        let edge = app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: 8, dy: sent.frame.midY))
        edge.press(forDuration: 0.05, thenDragTo: edge.withOffset(CGVector(dx: 70, dy: 2)),
                   withVelocity: XCUIGestureVelocity(300), thenHoldForDuration: 0.1)
        sleep(1)
        XCTAssertFalse(close.exists, "an edge drag never opens a thread")
    }
}
