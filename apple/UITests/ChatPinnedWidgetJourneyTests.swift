import XCTest

final class ChatPinnedWidgetJourneyTests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    @MainActor
    func testPinnedWidgetStaysAtTheTopWhileMessagesScrollAndStillOpens() {
        let app = XCUIApplication()
        app.launchArguments = ["-RodaDemo", "YES", "-RodaFreshStart", "YES", "-RodaResetDemo", "YES",
                               "-AppleLanguages", "(pt-BR)", "-RodaAppearance", "light",
                               "-RodaApprovalsExplained", "YES", "-RodaStory", "unread"]
        app.launch()

        // This story adds unread messages to Saturday Crew, which already has live apps.
        let strips = app.descendants(matching: .any).matching(identifier: "chat-pinned-apps")
        let strip = strips.firstMatch
        XCTAssertTrue(strip.waitForExistence(timeout: 20), "the chat has a pinned mini-app strip")
        XCTAssertEqual(strips.count, 1, "the strip appears once, above the scrolling history")
        let tiles = strip.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'pin-tile-' AND NOT (identifier BEGINSWITH 'pin-tile-remove') AND identifier != 'pin-tile-done'"))
        guard let tile = tiles.allElementsBoundByIndex.first(where: {
            $0.isHittable && $0.frame.width > 100 && $0.frame.height > 80
        }) else { return XCTFail("a live pinned tile is visible while opening unread history") }

        let message = app.staticTexts.matching(NSPredicate(
            format: "label BEGINSWITH %@", "Mensagem não lida 2:")).firstMatch
        expectation(for: NSPredicate(format: "exists == true AND hittable == true"), evaluatedWith: message)
        waitForExpectations(timeout: 15)
        let tileBefore = tile.frame
        let stripBefore = strip.frame
        let messageBefore = message.frame
        XCTAssertGreaterThanOrEqual(messageBefore.minY, stripBefore.maxY,
                                    "the fixed widgets reserve room above the actual messages")
        capture("Pinned widget before scrolling unread messages")

        let timeline = app.scrollViews["chat-message-scroll"].firstMatch
        XCTAssertTrue(timeline.exists, "the message scroll view is separate from the pinned strip")
        let scrollFrame = timeline.frame
        let startY = min(stripBefore.maxY + 180, scrollFrame.maxY - 120)
        let endY = max(stripBefore.maxY + 24, startY - 120)
        XCTAssertGreaterThan(startY - endY, 50, "there is room to scroll beneath the widgets")
        let start = timeline.coordinate(withNormalizedOffset: .zero)
            .withOffset(CGVector(dx: scrollFrame.width - 20, dy: startY - scrollFrame.minY))
        start.press(forDuration: 0.05, thenDragTo: start.withOffset(CGVector(dx: 0, dy: endY - startY)),
                    withVelocity: XCUIGestureVelocity(240), thenHoldForDuration: 0.2)

        XCTAssertGreaterThan(messageBefore.minY - message.frame.minY, 40,
                             "a real message moved when the history scrolled")
        XCTAssertTrue(tile.isHittable, "the live widget stays reachable after scrolling")
        XCTAssertEqual(tile.frame.minY, tileBefore.minY, accuracy: 3, "the tile remains fixed at the top")
        XCTAssertEqual(strip.frame.minY, stripBefore.minY, accuracy: 3)
        XCTAssertEqual(strip.frame.height, stripBefore.height, accuracy: 3,
                       "scrolling messages does not change the widget row's height")
        capture("Pinned widget held while a real message moves")

        tile.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        let miniApp = app.descendants(matching: .any)["miniapp-sheet"].firstMatch
        XCTAssertTrue(miniApp.waitForExistence(timeout: 10), "the pinned widget still opens its live mini-app")
        let denyNetwork = app.buttons["Don’t allow"].firstMatch
        let nativeClose = miniApp.buttons["miniapp-close"].firstMatch
        let webClose = miniApp.webViews.buttons.matching(NSPredicate(
            format: "label IN %@", ["Close", "Fechar"])).firstMatch
        expectation(for: NSPredicate { _, _ in
            denyNetwork.exists || nativeClose.exists || webClose.exists
        }, evaluatedWith: app)
        waitForExpectations(timeout: 15)
        if denyNetwork.exists { denyNetwork.tap() }
        let close = nativeClose.exists ? nativeClose : webClose
        XCTAssertTrue(close.waitForExistence(timeout: 10), "the app can close without granting network access")
        capture("Pinned widget opens its mini-app after scrolling")
        close.tap()
        expectation(for: NSPredicate(format: "exists == false"), evaluatedWith: miniApp)
        waitForExpectations(timeout: 10)
        XCTAssertTrue(tile.isHittable, "closing returns to the pinned widget in the chat")
        capture("Closing the pinned mini-app returns to the chat")
    }

    @MainActor
    private func capture(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
