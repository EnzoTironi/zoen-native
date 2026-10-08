import XCTest

/// Opens Spaces, creates a Space through the app, and sees it listed.
final class SpacesJourneyTests: XCTestCase {
    @MainActor
    func testCreateSpaceAndSeeItListed() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = [
            "-RodaFreshStart", "YES",
            "-RodaDemo", "YES",
            "-RodaTab", "spaces",
            "-AppleLanguages", "(en)",
            "-RodaAppearance", "light",
        ]
        app.launch()

        // Spaces tab is already selected via -RodaTab spaces.
        let create = app.buttons["createSpace"].exists
            ? app.buttons["createSpace"]
            : app.buttons["createSpaceEmpty"]
        XCTAssertTrue(create.waitForExistence(timeout: 15), "create control on Spaces")
        create.tap()

        let name = app.textFields["spaceNameField"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        name.tap()
        let title = "Trail Club \(Int(Date().timeIntervalSince1970) % 10_000)"
        name.typeText(title)

        app.buttons["confirmCreateSpace"].tap()

        // After create we open the Space; go back and confirm it's listed.
        let back = app.buttons.matching(NSPredicate(format: "label CONTAINS[c] 'back' OR identifier CONTAINS[c] 'back'")).firstMatch
        if back.waitForExistence(timeout: 8) { back.tap() }

        // Re-select Spaces if needed.
        let spacesTab = app.buttons["Spaces"]
        if spacesTab.waitForExistence(timeout: 3) { spacesTab.tap() }

        let row = app.descendants(matching: .any)["spaceRow-\(title)"]
        let listed = row.waitForExistence(timeout: 10)
            || app.staticTexts[title].waitForExistence(timeout: 2)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = listed ? "space-listed" : "space-missing"
        shot.lifetime = .keepAlways
        add(shot)
        XCTAssertTrue(listed, "the new Space appears in the list")
    }
}
