import XCTest

/// Matched seeded screens for the Android Material 3 / iOS Liquid Glass comparison.
final class AndroidParityCaptureTests: XCTestCase {
    @MainActor func testLightScreens() throws { try capture(dark: false) }
    @MainActor func testDarkScreens() throws { try capture(dark: true) }
    @MainActor func testOnboardingLightFrame() throws { try capture(dark: false, screens: ["onboarding"]) }
    @MainActor func testIntegrationLightScreens() throws { try capture(dark: false, screens: ["home", "marketplace", "notifications", "search"]) }
    @MainActor func testIntegrationDarkScreens() throws { try capture(dark: true, screens: ["home", "marketplace", "notifications", "search"]) }
    @MainActor func testIntegrationDarkSearch() throws { try capture(dark: true, screens: ["search"]) }

    @MainActor private func capture(dark: Bool, screens: [String] = ["onboarding", "chats", "chat", "composer", "spaces", "notifications", "profile", "settings"]) throws {
        continueAfterFailure = false
        let environment = ProcessInfo.processInfo.environment
        guard environment["ZOEN_PARITY_FIXTURE"] != nil else {
            throw XCTSkip("Native parity capture requires explicit fixture opt-in")
        }
        XCUIDevice.shared.orientation = environment["ZOEN_PARITY_VIEWPORT"] == "wide" ? .landscapeLeft : .portrait
        let fixture = environment["ZOEN_PARITY_FIXTURE"] ?? ""
        let revision = environment["ZOEN_PARITY_REVISION"] ?? ""
        XCTAssertEqual(fixture, "native-parity-v1")
        XCTAssertNotNil(revision.range(of: "^[0-9a-f]{40}$", options: .regularExpression))
        var appliedManifest: String?
        let appearance = dark ? "dark" : "light"
        for screen in screens {
            let app = XCUIApplication()
            app.launchEnvironment["ZOEN_PARITY_FIXTURE"] = fixture
            app.launchEnvironment["ZOEN_PARITY_REVISION"] = revision
            app.launchEnvironment["ZOEN_PARITY_VIEWPORT"] = environment["ZOEN_PARITY_VIEWPORT"]
            app.launchEnvironment["TZ"] = "UTC"
            app.launchArguments = ["-RodaFreshStart", "YES", "-RodaResetDemo", "YES",
                "-AppleLanguages", "(en)", "-AppleLocale", "en_US", "-RodaAppearance", appearance,
                "-RodaDemo", "YES"]
            if screen == "onboarding" {
                app.launchArguments += ["-RodaOnboarding", "YES"]
            } else {
                app.launchArguments += ["-RodaDemo", "YES"]
                if screen == "settings" || screen == "profile" { app.launchArguments += ["-RodaTab", "you"] }
            }
            app.launch()
            let attestation = app.descendants(matching: .any)["parity-applied-fixture"].firstMatch
            XCTAssertTrue(attestation.waitForExistence(timeout: 30), "Core seed, IDs, avatar, clock or locale fixture was not applied")
            let actual = attestation.label
            let manifest = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(actual.utf8)) as? [String: Any])
            XCTAssertEqual(manifest["fixture_id"] as? String, fixture)
            XCTAssertEqual(manifest["source_revision"] as? String, revision)
            if let previous = appliedManifest { XCTAssertEqual(actual, previous) }
            else {
                appliedManifest = actual
                let attachment = XCTAttachment(data: Data(actual.utf8), uniformTypeIdentifier: "public.json")
                attachment.name = "parity-fixture.json"
                attachment.lifetime = .keepAlways
                add(attachment)
            }
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
                // Capture the current seeded community journey; the Files/Spaces root tab
                // was removed when compact navigation moved to Chats, Marketplace and Profile.
                let community = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Coastal Travelers")).firstMatch
                XCTAssertTrue(community.waitForExistence(timeout: 30), "The seeded community must be reachable from the current Chats root")
                community.tap()
                XCTAssertTrue(app.buttons["chat-title"].waitForExistence(timeout: 10))
            } else if screen == "profile" {
                XCTAssertTrue(app.buttons["openSettings"].waitForExistence(timeout: 30))
            } else if screen == "marketplace" {
                let marketplace = app.tabBars.buttons["Marketplace"].firstMatch
                XCTAssertTrue(marketplace.waitForExistence(timeout: 30))
                marketplace.tap()
            } else if screen == "search" {
                let search = app.tabBars.buttons["Search"].firstMatch
                XCTAssertTrue(search.waitForExistence(timeout: 30))
                search.tap()
                XCTAssertTrue(app.textFields["unified-search-field"].waitForExistence(timeout: 10))
                // A fresh simulator can show Apple's slide-to-type introduction.
                // Dismiss the observed system prompt rather than capturing it over Search.
                let keyboardIntroduction = app.staticTexts["Speed up your typing by sliding your finger across the letters to compose a word."]
                if keyboardIntroduction.waitForExistence(timeout: 3) {
                    app.buttons["Continue"].tap()
                    XCTAssertFalse(keyboardIntroduction.waitForExistence(timeout: 1))
                }
            } else if screen == "notifications" {
                let notifications = app.descendants(matching: .any)["home-notifications-card"].firstMatch
                XCTAssertTrue(notifications.waitForExistence(timeout: 30), "Notifications must remain reachable from the first Home card")
                notifications.tap()
                XCTAssertTrue(app.descendants(matching: .any)["notifications-list-view"].firstMatch.waitForExistence(timeout: 10))
            } else {
                let searchAction: XCUIElement
                if environment["ZOEN_PARITY_VIEWPORT"] == "wide" {
                    searchAction = app.buttons["Search"].firstMatch
                } else {
                    let bottomBar = app.tabBars.firstMatch
                    XCTAssertTrue(bottomBar.waitForExistence(timeout: 30), "Compact root navigation must stay in the bottom bar")
                    searchAction = bottomBar.buttons["Search"].firstMatch
                }
                XCTAssertTrue(searchAction.waitForExistence(timeout: 30), "The bottom/rail Search action must remain available")
                XCTAssertFalse(app.navigationBars.buttons["Search"].exists, "Root actions must not return to the top bar")
            }
            // Await app idleness and one settled animation before recording the actual app frame.
            Thread.sleep(forTimeInterval: screen == "onboarding" ? 8 : 2)
            // Diagnostic geometry is separate from the common cross-platform fixture contract.
            let geometryWitness = app.descendants(matching: .any)["parity-window-geometry"].firstMatch
            XCTAssertTrue(geometryWitness.waitForExistence(timeout: 10), "Applied fixture requires actual UIKit window geometry")
            var geometry = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(geometryWitness.label.utf8)) as? [String: Any])
            XCTAssertNotNil(geometry["window_points"] as? [String: Any])
            XCTAssertNotNil(geometry["safe_area_points"] as? [String: Any])
            XCTAssertNotNil(geometry["safe_area_pixels"] as? [String: Any])
            XCTAssertGreaterThan(try XCTUnwrap(geometry["scale"] as? Double), 0)
            geometry["screen"] = screen
            geometry["mode"] = appearance
            geometry["viewport"] = environment["ZOEN_PARITY_VIEWPORT"] ?? "narrow"
            let diagnostic = XCTAttachment(data: try JSONSerialization.data(withJSONObject: geometry, options: [.sortedKeys]), uniformTypeIdentifier: "public.json")
            diagnostic.name = "ios-geometry-\(screen)-\(appearance).json"
            diagnostic.lifetime = .keepAlways
            add(diagnostic)
            let shot = XCTAttachment(screenshot: app.screenshot())
            shot.name = "ios-\(screen)-\(appearance)"
            shot.lifetime = .keepAlways
            add(shot)
            app.terminate()
        }
    }
}
