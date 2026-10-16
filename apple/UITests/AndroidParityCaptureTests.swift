import XCTest

/// Matched seeded screens for the Android Material 3 / iOS Liquid Glass comparison.
final class AndroidParityCaptureTests: XCTestCase {
    @MainActor func testLightScreens() throws { try capture(dark: false) }
    @MainActor func testDarkScreens() throws { try capture(dark: true) }
    @MainActor func testOnboardingLightFrame() throws { try capture(dark: false, screens: ["onboarding"]) }

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
                if screen == "spaces" { app.launchArguments += ["-RodaTab", "files"] }
                if screen == "notifications" { app.launchArguments += ["-RodaTab", "activity"] }
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
                XCTAssertTrue(app.descendants(matching: .any)["spaces-directory"].firstMatch.waitForExistence(timeout: 30))
            } else if screen == "profile" {
                XCTAssertTrue(app.buttons["openSettings"].waitForExistence(timeout: 30))
            } else if screen == "notifications" {
                XCTAssertTrue(app.staticTexts["Notifications"].firstMatch.waitForExistence(timeout: 30))
            } else {
                XCTAssertTrue(app.buttons["New message"].firstMatch.waitForExistence(timeout: 30))
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
