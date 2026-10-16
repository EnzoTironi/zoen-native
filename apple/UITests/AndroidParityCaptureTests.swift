import XCTest

/// Matched seeded screens for the Android Material 3 / iOS Liquid Glass comparison.
final class AndroidParityCaptureTests: XCTestCase {
    @MainActor func testLightScreens() { capture(dark: false) }
    @MainActor func testDarkScreens() { capture(dark: true) }
    @MainActor func testOnboardingLightFrame() { capture(dark: false, screens: ["onboarding"]) }

    @MainActor private func capture(dark: Bool, screens: [String] = ["onboarding", "chats", "chat", "composer", "spaces", "settings"]) {
        continueAfterFailure = false
        let environment = ProcessInfo.processInfo.environment
        XCUIDevice.shared.orientation = environment["ZOEN_PARITY_VIEWPORT"] == "wide" ? .landscapeLeft : .portrait
        let fixture = environment["ZOEN_PARITY_FIXTURE"] ?? ""
        let revision = environment["ZOEN_PARITY_REVISION"] ?? ""
        XCTAssertEqual(fixture, "native-parity-v1")
        XCTAssertNotNil(revision.range(of: "^[0-9a-f]{40}$", options: .regularExpression))
        let manifest: [String: Any] = ["fixture_id": fixture, "fixed_time": "2026-01-15T12:00:00Z",
            "locale": "en_US", "timezone": "UTC", "seed": 128, "source_revision": revision]
        let attachment = XCTAttachment(data: try! JSONSerialization.data(withJSONObject: manifest, options: [.sortedKeys]), uniformTypeIdentifier: "public.json")
        attachment.name = "parity-fixture.json"
        attachment.lifetime = .keepAlways
        add(attachment)
        let appearance = dark ? "dark" : "light"
        for screen in screens {
            let app = XCUIApplication()
            app.launchEnvironment["ZOEN_PARITY_FIXTURE"] = fixture
            app.launchEnvironment["ZOEN_PARITY_REVISION"] = revision
            app.launchEnvironment["TZ"] = "UTC"
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
                let greeting = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "Hi, I")).firstMatch
                XCTAssertTrue(greeting.waitForExistence(timeout: 30))
                XCTAssertTrue(app.buttons["Hi, Zoen!"].waitForExistence(timeout: 10))
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
            Thread.sleep(forTimeInterval: screen == "onboarding" ? 8 : 2)
            let shot = XCTAttachment(screenshot: app.screenshot())
            shot.name = "ios-\(screen)-\(appearance)"
            shot.lifetime = .keepAlways
            add(shot)
            app.terminate()
        }
    }
}
