import XCTest

/// The chat title grows into a menu (on for everyone).
final class HeaderMenuTests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    @MainActor private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-RodaDemo", "YES", "-RodaFreshStart", "YES", "-RodaResetDemo", "YES",
                               "-AppleLanguages", "(pt-BR)", "-RodaAppearance", "light",
                               "-RodaApprovalsExplained", "YES", "-RodaOpen", "paraty"]
        app.launch()
        return app
    }

    @MainActor
    func testTitleExpandsIntoMenu() {
        let app = launch()
        let title = app.buttons["chat-title"]
        XCTAssertTrue(title.waitForExistence(timeout: 20))
        sleep(2)
        title.tap()
        let menu = app.descendants(matching: .any)["header-menu"]
        XCTAssertTrue(menu.waitForExistence(timeout: 3), "the header grows into the menu")
        for row in ["members", "files", "pages", "agents", "mute", "settings"] {
            XCTAssertTrue(app.buttons["header-menu-\(row)"].exists, "menu has \(row)")
        }
        XCTAssertTrue(app.buttons["Membros"].exists, "rows are in Portuguese")
        sleep(1)
        // A tap on the dimmed chat folds it back.
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.88)).tap()
        XCTAssertTrue(menu.waitForNonExistence(timeout: 3), "tapping outside closes it")
        sleep(1)
        // Title again, then Membros goes to the people in the chat.
        title.tap()
        XCTAssertTrue(app.buttons["header-menu-members"].waitForExistence(timeout: 3))
        sleep(1)
        app.buttons["header-menu-members"].tap()
        XCTAssertTrue(menu.waitForNonExistence(timeout: 3))
        XCTAssertTrue(app.staticTexts["Marina"].firstMatch.waitForExistence(timeout: 5), "members list")
    }
}
