import XCTest

/// Tapping a mini-app tile flips it over into the full-screen app; closing flips it back
/// into the same tile, on the same screen (Home stays Home).
final class MiniAppFlipJourneyTests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    @MainActor
    func testHomeTileFlipsOpenAndBackIntoTheTile() {
        let app = XCUIApplication()
        app.launchArguments = ["-RodaDemo", "YES", "-RodaShowcase", "YES", "-AppleLanguages", "(pt-BR)",
                               "-RodaAppearance", "light", "-RodaApprovalsExplained", "YES",
                               "-RodaFreshStart", "YES", "-RodaResetDemo", "YES"]
        app.launch()
        let tiles = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'home-tile-'"))
        XCTAssertTrue(tiles.firstMatch.waitForExistence(timeout: 20), "Home shows mini-app tiles")
        sleep(2)
        // A native mini-app (the pet or the countdown) so its own close button is there.
        let width = app.windows.firstMatch.frame.width
        let candidates = app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH 'home-tile-' AND (identifier == 'home-tile-Paraty' OR identifier CONTAINS 'Paçoca')"))
        // The copy on screen (another Home can sit off to the side).
        func onScreen() -> XCUIElement? {
            candidates.allElementsBoundByIndex.first { $0.exists && $0.frame.minX > -8 && $0.frame.minX < width && $0.frame.width > 60 }
        }
        var found = onScreen()
        for _ in 0..<5 where found == nil || found!.frame.maxX > width {
            app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'home-tile-'"))
                .allElementsBoundByIndex.first { $0.frame.minX > -8 && $0.frame.minX < width }?.swipeLeft()
            sleep(1)
            found = onScreen()
        }
        guard let tile = found else { return XCTFail("the pet or countdown tile is on Home") }
        XCTAssertTrue(tile.isHittable)
        let before = tile.frame

        tile.tap()
        sleep(1)
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "flip-open"; shot.lifetime = .keepAlways; add(shot)
        let sheet = app.descendants(matching: .any)["miniapp-sheet"]
        XCTAssertTrue(sheet.waitForExistence(timeout: 5), "the tile turns into the mini-app")
        let close = app.buttons["miniapp-close"].firstMatch
        XCTAssertTrue(close.waitForExistence(timeout: 5), "the app is live, with its close button")
        sleep(1)
        let full = sheet.frame
        XCTAssertGreaterThan(full.width, before.width * 1.8, "it grew to the screen")
        sleep(1)

        close.tap()
        XCTAssertTrue(sheet.waitForNonExistence(timeout: 5), "closing flips it away")
        XCTAssertTrue(tile.waitForExistence(timeout: 3))
        sleep(1)
        XCTAssertTrue(tile.isHittable, "back on Home, the tile is where it was")
        XCTAssertEqual(tile.frame.midX, before.midX, accuracy: 4)
        XCTAssertFalse(app.buttons["zoenBack"].exists, "opening from Home didn't jump into the chat")

        // And again: it opens every time (no state left behind).
        tile.tap()
        XCTAssertTrue(close.waitForExistence(timeout: 6), "opens again")
        close.tap()
        XCTAssertTrue(sheet.waitForNonExistence(timeout: 5))
    }
}
