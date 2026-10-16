import XCTest

/// Matched seeded screens for the Android Material 3 / iOS Liquid Glass comparison.
final class AndroidParityCaptureTests: XCTestCase {
    @MainActor func testLightScreens() { capture(dark: false) }
    @MainActor func testDarkScreens() { capture(dark: true) }

    @MainActor private func capture(dark: Bool) {
        continueAfterFailure = false
        let appearance = dark ? "dark" : "light"
        for screen in ["onboarding", "chats", "chat", "composer", "spaces", "settings"] {
            let app = XCUIApplication()
            app.launchArguments = ["-RodaFreshStart", "YES", "-RodaResetDemo", "YES",
                "-AppleLanguages", "(en)", "-AppleLocale", "en_US", "-RodaAppearance", appearance]
            if screen == "onboarding" {
                app.launchArguments += ["-RodaOnboarding", "YES"]
            } else {
                app.launchArguments += ["-RodaDemo", "YES"]
                if screen == "spaces" { app.launchArguments += ["-RodaOpen", "coastal"] }
                if screen == "settings" { app.launchArguments += ["-RodaTab", "you"] }
            }
            app.launch()
            if screen == "onboarding" {
                XCTAssertTrue(app.buttons.firstMatch.waitForExistence(timeout: 30))
            } else if screen == "settings" {
                let settings = app.buttons["openSettings"]
                XCTAssertTrue(settings.waitForExistence(timeout: 30))
                settings.tap()
                XCTAssertTrue(app.textFields["settingsSearch"].waitForExistence(timeout: 10))
            } else if screen == "chat" || screen == "composer" {
                let marina = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Marina")).firstMatch
                XCTAssertTrue(marina.waitForExistence(timeout: 30))
                marina.tap()
                XCTAssertTrue(app.buttons["chat-title"].waitForExistence(timeout: 10))
                if screen == "composer" {
                    let composer = app.descendants(matching: .any)["composer"].firstMatch
                    XCTAssertTrue(composer.waitForExistence(timeout: 10))
                    composer.tap()
                    composer.typeText("See you tomorrow")
                    XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 10))
                }
            } else if screen == "spaces" {
                XCTAssertTrue(app.buttons["chat-title"].waitForExistence(timeout: 30))
            } else {
                XCTAssertTrue(app.buttons["newChat"].waitForExistence(timeout: 30))
            }
            // Await app idleness and one settled animation before recording the actual app frame.
            Thread.sleep(forTimeInterval: 2)
            let shot = XCTAttachment(screenshot: app.screenshot())
            shot.name = "ios-\(screen)-\(appearance)"
            shot.lifetime = .keepAlways
            add(shot)
            app.terminate()
        }
    }
}
