import XCTest
import UIKit

/// Screenshots are captured from real components and real demo Core mutations.
final class LivingSpaceCaptureTests: XCTestCase {
    @MainActor func testBaselineLight() throws { try baseline("light") }
    @MainActor func testBaselineDark() throws { try baseline("dark") }
    @MainActor func testExpandedLight() throws { try expanded("light") }
    @MainActor func testExpandedDark() throws { try expanded("dark") }

    @MainActor func testExperimentOffRetainsExistingChat() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-LivingSpacesEnabled", "NO", "-RodaDemo", "YES", "-RodaLivingSpaceFixture", "YES",
            "-RodaLivingFixtureReset", "YES", "-RodaOpen", "paraty", "-AppleLanguages", "(en)"]
        app.launch()
        XCTAssertTrue(app.scrollViews["chat-message-scroll"].waitForExistence(timeout: 45))
        XCTAssertFalse(app.scrollViews["living-space.home"].exists)
        XCTAssertFalse(app.tabBars.firstMatch.exists)
    }

    @MainActor private func baseline(_ theme: String) throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = [
            "-RodaDemo", "YES", "-LivingSpacesEnabled", "YES", "-RodaLivingSpaceFixture", "YES",
            "-RodaLivingSpaceScreen", "home", "-AppleLanguages", "(en)", "-AppleLocale", "en_US",
            "-RodaAppearance", theme, "-RodaLivingFixtureVariant", theme,
            "-RodaLivingFixtureReset", "YES", "-RodaLivingJoinFixture", "YES",
        ]
        XCUIDevice.shared.orientation = UIDevice.current.userInterfaceIdiom == .pad ? .landscapeLeft : .portrait
        app.launch()
        let home = app.scrollViews["living-space.home"]
        XCTAssertTrue(home.waitForExistence(timeout: 45))
        let announcements = app.buttons["living-space.announcements.open"]
        XCTAssertTrue(announcements.waitForExistence(timeout: 15))
        capture(app, theme, "home")
        announcements.tap()
        XCTAssertTrue(app.navigationBars["Announcements"].waitForExistence(timeout: 5))
        capture(app, theme, "announcements")
        app.buttons["Done"].tap()
        let page = app.buttons["living-space.page.open"]
        scrollTo(page, in: app)
        XCTAssertTrue(page.waitForExistence(timeout: 10))
        page.tap()
        let editor = app.textViews["page.editor"].firstMatch
        XCTAssertTrue(editor.waitForExistence(timeout: 10))
        XCTAssertFalse(app.staticTexts["page.saveError"].exists)
        let marker = " Fixture edit persists."
        editor.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.75)).tap()
        editor.typeText(marker)
        let saved = app.descendants(matching: .any).matching(NSPredicate(format: "identifier IN %@", ["page.saved", "page.pendingSync"])).firstMatch
        XCTAssertTrue(saved.waitForExistence(timeout: 10))
        app.navigationBars.buttons["Home"].firstMatch.tap()
        scrollTo(page, in: app)
        page.tap()
        XCTAssertTrue(editor.waitForExistence(timeout: 10))
        XCTAssertTrue((editor.value as? String)?.contains(marker) == true, "actual page edit must persist after leaving and reopening")
        editor.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.75)).tap()
        editor.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: marker.count))
        XCTAssertTrue(saved.waitForExistence(timeout: 10))
        app.navigationBars.buttons["Home"].firstMatch.tap()
        scrollTo(page, in: app)
        page.tap()
        XCTAssertTrue(editor.waitForExistence(timeout: 10))
        XCTAssertFalse((editor.value as? String)?.contains(marker) == true)
        XCTAssertFalse(app.staticTexts["page.saveError"].exists)
        capture(app, theme, "page")
        app.navigationBars.buttons["Home"].firstMatch.tap()
        let notifications = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Notifications")).firstMatch
        XCTAssertTrue(notifications.waitForExistence(timeout: 5))
        notifications.tap()
        XCTAssertTrue(app.navigationBars["Notifications"].waitForExistence(timeout: 5))
        let approve = app.buttons["Approve"].firstMatch
        XCTAssertTrue(approve.waitForExistence(timeout: 10), "a signed synthetic requester must be projected by Core")
        capture(app, theme, "notifications")
        approve.tap()
        let gone = expectation(for: NSPredicate(format: "exists == false"), evaluatedWith: approve)
        wait(for: [gone], timeout: 10)
        app.buttons["Spaces"].firstMatch.tap()
        let memberCount = app.staticTexts["2 members"]
        XCTAssertTrue(memberCount.waitForExistence(timeout: 10), "approval must add the signed requester to membership")
    }

    @MainActor private func expanded(_ theme: String) throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = [
            "-RodaDemo", "NO", "-LivingSpacesEnabled", "YES", "-RodaLivingAccountFixture", "YES",
            "-RodaLivingExpandedFixture", "YES", "-RodaLivingSpaceScreen", "home",
            "-AppleLanguages", "(en)", "-AppleLocale", "en_US", "-RodaAppearance", theme,
            "-RodaLivingFixtureVariant", theme,
        ]
        XCUIDevice.shared.orientation = UIDevice.current.userInterfaceIdiom == .pad ? .landscapeLeft : .portrait
        app.launch()
        let bootError = app.staticTexts.matching(identifier: "living-space.boot-error")
        if bootError.firstMatch.waitForExistence(timeout: 5) {
            XCTFail(bootError.allElementsBoundByIndex.map(\.label).joined(separator: " · "))
            return
        }
        XCTAssertTrue(app.scrollViews["living-space.home"].waitForExistence(timeout: 45))
        let hackathon = app.buttons["living-space.hackathon.open"]
        scrollTo(hackathon, in: app)
        XCTAssertTrue(hackathon.waitForExistence(timeout: 15))
        hackathon.tap()
        XCTAssertTrue(app.staticTexts["Build together"].waitForExistence(timeout: 10))
        capture(app, theme, "hackathon")
        let team = app.buttons["Encrypted team conversation"]
        scrollTo(team, in: app)
        XCTAssertTrue(team.exists, "team must come from a real MLS group, not a fabricated view model")
        capture(app, theme, "teams")
        team.tap()
        let composer = app.descendants(matching: .any)["living-space.message"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))
        composer.tap()
        composer.typeText("Ready to build together.")
        app.buttons["living-space.send"].tap()
        XCTAssertTrue(app.staticTexts["Ready to build together."].waitForExistence(timeout: 10))
        XCTAssertFalse(app.otherElements["agent-browser"].exists)
        app.navigationBars.buttons["Hackathon"].firstMatch.tap()
        let submission = app.buttons["Submit project"]
        scrollTo(submission, in: app)
        XCTAssertTrue(submission.exists)
        XCTAssertTrue(app.links["Submitted project"].exists || app.buttons["Submitted project"].exists)
        capture(app, theme, "submission")
        let publish = app.buttons["Publish winners announcement"]
        scrollTo(publish, in: app)
        if publish.exists {
            let enabled = NSPredicate(format: "enabled == true")
            expectation(for: enabled, evaluatedWith: publish)
            waitForExpectations(timeout: 115)
            publish.tap()
        }
        let published = app.staticTexts["Results published to announcements"]
        XCTAssertTrue(published.waitForExistence(timeout: 10))
        capture(app, theme, "results")
        let vote = app.buttons["Vote"].firstMatch
        scrollTo(vote, in: app)
        XCTAssertTrue(vote.exists)
        XCTAssertFalse(vote.isEnabled, "finalized/self-team voting must be disabled, regardless of tint")
        let judge = app.buttons["Submit judge score"].firstMatch
        XCTAssertTrue(judge.exists)
        XCTAssertFalse(judge.isEnabled, "finalized/self-team scoring must be disabled")
        app.navigationBars.buttons["Home"].firstMatch.tap()
        let tiers = app.buttons["living-space.tiers.open"]
        scrollTo(tiers, in: app)
        XCTAssertTrue(tiers.exists)
        tiers.tap()
        XCTAssertTrue(app.staticTexts["Builder"].firstMatch.waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["Administrator access"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["Revoke"].exists, "administrator access cannot be revoked through memberships")
        let createInvitation = app.buttons["Create invitation"].firstMatch
        scrollTo(createInvitation, in: app)
        XCTAssertTrue(createInvitation.exists)
        createInvitation.tap()
        let shareInvitation = app.buttons["Share invitation"]
        scrollTo(shareInvitation, in: app)
        XCTAssertTrue(shareInvitation.waitForExistence(timeout: 10))
        // Re-enter to discard screen-local invitation state; metadata must survive reload.
        app.navigationBars.buttons["Home"].firstMatch.tap()
        scrollTo(tiers, in: app)
        tiers.tap()
        let revoke = app.buttons["living-space.invitation.revoke"].firstMatch
        scrollTo(revoke, in: app)
        XCTAssertTrue(revoke.waitForExistence(timeout: 10))
        capture(app, theme, "tiers")
        revoke.tap()
        XCTAssertTrue(app.staticTexts["Revoked"].waitForExistence(timeout: 10))
    }

    @MainActor private func scrollTo(_ element: XCUIElement, in app: XCUIApplication) {
        for _ in 0..<8 {
            if element.exists && element.isHittable { return }
            app.swipeUp()
        }
    }

    @MainActor private func capture(_ app: XCUIApplication, _ theme: String, _ screen: String) {
        let device = UIDevice.current.userInterfaceIdiom == .pad ? "ipad" : "iphone"
        let screenshot = XCUIScreen.main.screenshot()
        let name = "\(device)-\(theme)-\(screen)"
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("LivingSpaceCaptures", isDirectory: true)
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try? screenshot.pngRepresentation.write(to: directory.appendingPathComponent(name + ".png"), options: .atomic)
        let attachment = XCTAttachment(screenshot: screenshot)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
        var frames: [String: [String: Double]] = [:]
        func record(_ name: String, _ element: XCUIElement) {
            guard element.exists else { return }
            let r = element.frame
            frames[name] = ["x": r.minX, "y": r.minY, "width": r.width, "height": r.height]
        }
        record("viewport", app)
        record("navigation", app.navigationBars.firstMatch)
        if screen == "home" {
            record("title", app.staticTexts["Living Space"].firstMatch)
            record("announcements", app.buttons["living-space.announcements.open"])
            record("page", app.buttons["living-space.page.open"])
            record("events", app.staticTexts["Events"].firstMatch)
            record("files", app.staticTexts["Files"].firstMatch)
            record("tabs", app.tabBars.firstMatch)
            record("sidebar", app.collectionViews.matching(NSPredicate(format: "label == %@", "Sidebar")).firstMatch)
        }
        if let data = try? JSONSerialization.data(withJSONObject: frames, options: [.sortedKeys]) {
            try? data.write(to: directory.appendingPathComponent(name + "-metrics.json"), options: .atomic)
            let metrics = XCTAttachment(data: data, uniformTypeIdentifier: "public.json")
            metrics.name = "\(device)-\(theme)-\(screen)-metrics"
            metrics.lifetime = .keepAlways
            add(metrics)
        }
    }
}
