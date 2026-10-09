import XCTest

/// Creates a community from Chats and finds it in the same inbox.
final class SpacesJourneyTests: XCTestCase {
    @MainActor
    func testCreateCommunityFromChatsAndSeeItInTheInbox() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = [
            "-RodaFreshStart", "YES",
            "-RodaDemo", "YES",
            "-RodaTab", "chats",
            "-AppleLanguages", "(en)",
            "-RodaAppearance", "light",
        ]
        app.launch()

        XCTAssertFalse(app.buttons["Spaces"].exists, "communities do not have a second inbox")
        let newChat = app.buttons["newChat"]
        XCTAssertTrue(newChat.waitForExistence(timeout: 15))
        newChat.tap()
        app.buttons["Community"].tap()

        let name = app.textFields["communityNameField"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        name.tap()
        let title = "Trail Club \(Int(Date().timeIntervalSince1970) % 10_000)"
        name.typeText(title)

        app.buttons["confirmCreateCommunity"].tap()

        let back = app.buttons["zoenBack"]
        XCTAssertTrue(back.waitForExistence(timeout: 10), "the new community opens as a conversation")
        back.tap()

        let row = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", title)).firstMatch
        let listed = row.waitForExistence(timeout: 10)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = listed ? "community-in-chats" : "community-missing-from-chats"
        shot.lifetime = .keepAlways
        add(shot)
        XCTAssertTrue(listed, "the community appears in Chats alongside direct conversations and groups")
    }
}
