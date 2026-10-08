import XCTest

/// Back must pop one screen — never open a history menu of destinations.
final class BackPopTests: XCTestCase {
    @MainActor
    func testBackFromChatPopsToList() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = [
            "-RodaFreshStart", "YES", "-RodaDemo", "YES",
            "-RodaOpen", "zoen", "-AppleLanguages", "(en)", "-RodaAppearance", "light",
        ]
        app.launch()

        let back = app.buttons["zoenBack"]
        XCTAssertTrue(back.waitForExistence(timeout: 15), "chat glass Back is on screen")
        // A menu would add other buttons; after a plain pop the chat composer is gone.
        back.tap()
        // Give any illicit menu a moment; it must NOT appear.
        let menu = app.menus.firstMatch
        XCTAssertFalse(menu.waitForExistence(timeout: 0.8), "Back must not open a destination menu")
        XCTAssertFalse(app.buttons["zoenBack"].waitForExistence(timeout: 3), "left the chat")
        // Home wordmark / chats list is back.
        XCTAssertTrue(
            app.staticTexts["zoen"].waitForExistence(timeout: 5)
                || app.buttons["newChat"].waitForExistence(timeout: 2),
            "landed on the chats list"
        )
    }

    @MainActor
    func testBackFromPlanPopsOnce() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = [
            "-RodaFreshStart", "YES", "-RodaDemo", "YES",
            "-RodaOpen", "paraty-plan", "-AppleLanguages", "(en)",
        ]
        app.launch()

        let back = app.buttons["zoenBack"]
        XCTAssertTrue(back.waitForExistence(timeout: 15), "plan screen Back")
        back.tap()
        XCTAssertFalse(app.menus.firstMatch.waitForExistence(timeout: 0.8), "no history menu")
        // One pop: still in the Paraty chat (glass Back + call controls), not all the way home.
        let chatBack = app.buttons["zoenBack"]
        XCTAssertTrue(chatBack.waitForExistence(timeout: 5), "popped to the chat, not the root")
        XCTAssertTrue(
            app.buttons["Voice call"].exists || app.buttons["Video call"].exists
                || app.descendants(matching: .any)["composer"].exists,
            "chat chrome is visible after one pop"
        )
    }
}
