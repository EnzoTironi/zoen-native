import XCTest

/// The chat hides the system navigation bar for its own glass top bar; the edge swipe back
/// must still pop to the chat list.
final class SwipeBackTests: XCTestCase {
    @MainActor
    func testEdgeSwipeGoesBackFromChat() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-RodaResetDemo", "YES", "-AppleLanguages", "(en)", "-RodaStory", "hike-ride", "-RodaAppAITimeout", "0"]
        app.launch()
        let call = app.buttons["Voice call"]
        XCTAssertTrue(call.waitForExistence(timeout: 15), "chat didn't open")

        let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.0, dy: 0.5))
        let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5))
        start.press(forDuration: 0.05, thenDragTo: end, withVelocity: .fast, thenHoldForDuration: 0.05)

        let gone = NSPredicate(format: "exists == false")
        expectation(for: gone, evaluatedWith: call)
        waitForExpectations(timeout: 5)
        let shot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        shot.name = "after-swipe"
        shot.lifetime = .keepAlways
        add(shot)
    }
}
