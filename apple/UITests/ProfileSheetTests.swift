import XCTest

final class ProfileSheetTests: XCTestCase {
    @MainActor
    func testTapSenderOpensProfileThenMessage() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = [
            "-RodaFreshStart", "YES", "-RodaDemo", "YES", "-RodaResetDemo", "YES",
            "-RodaOpen", "paraty", "-AppleLanguages", "(en)",
        ]
        app.launch()

        // Group chat with other people — tap a sender avatar / name.
        // Wait for a message from Marina (demo).
        let marina = app.staticTexts["Marina"].firstMatch
        XCTAssertTrue(marina.waitForExistence(timeout: 15), "group chat shows Marina")
        marina.tap()

        // Profile sheet medium detent
        XCTAssertTrue(app.buttons["Message"].firstMatch.waitForExistence(timeout: 5), "profile sheet actions")
        app.buttons["Message"].firstMatch.tap()

        // Lands in a chat (1:1 or existing space with them)
        XCTAssertTrue(app.buttons["zoenBack"].waitForExistence(timeout: 8)
                      || app.descendants(matching: .any)["composer"].waitForExistence(timeout: 5),
                      "opened a chat from Message")
    }
}
