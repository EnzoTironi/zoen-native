import XCTest

/// Extra matched full-device frames for the floating bar and the live composer.
/// These are opt-in evidence tests, not substitutes for voice/media journey tests.
final class BottomComposerParityCaptureTests: XCTestCase {
    @MainActor func testLightFrames() throws { try capture(dark: false) }
    @MainActor func testDarkFrames() throws { try capture(dark: true) }

    @MainActor private func capture(dark: Bool) throws {
        let environment = ProcessInfo.processInfo.environment
        guard let fixture = environment["ZOEN_PARITY_FIXTURE"] else {
            throw XCTSkip("Run explicitly with the native parity fixture")
        }
        let revision = try XCTUnwrap(environment["ZOEN_PARITY_REVISION"])
        XCTAssertEqual(fixture, "native-parity-v1")
        XCTAssertNotNil(revision.range(of: "^[0-9a-f]{40}$", options: .regularExpression))
        continueAfterFailure = false
        XCUIDevice.shared.orientation = .portrait
        let appearance = dark ? "dark" : "light"
        var appliedFixture: String?
        for frame in ["chats", "composer-empty", "attachment-menu", "composer-text"] {
            let app = XCUIApplication()
            app.launchEnvironment["ZOEN_PARITY_FIXTURE"] = fixture
            app.launchEnvironment["ZOEN_PARITY_REVISION"] = revision
            app.launchEnvironment["TZ"] = "UTC"
            app.launchArguments = ["-RodaFreshStart", "YES", "-RodaResetDemo", "YES", "-RodaDemo", "YES",
                "-AppleLanguages", "(en)", "-AppleLocale", "en_US", "-RodaAppearance", appearance]
            app.launch()
            let witness = app.descendants(matching: .any)["parity-applied-fixture"].firstMatch
            XCTAssertTrue(witness.waitForExistence(timeout: 40), "The app must attest its applied core fixture")
            let manifest = witness.label
            if let appliedFixture { XCTAssertEqual(manifest, appliedFixture) }
            else { appliedFixture = manifest }
            let metadata = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(manifest.utf8)) as? [String: Any])
            XCTAssertEqual(metadata["fixture_id"] as? String, fixture)
            XCTAssertEqual(metadata["source_revision"] as? String, revision)
            XCTAssertEqual(metadata["fixed_time"] as? String, "2026-01-15T12:00:00Z")
            XCTAssertTrue(app.buttons["newChat"].waitForExistence(timeout: 30))
            if frame != "chats" {
                let marina = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Marina")).firstMatch
                XCTAssertTrue(marina.waitForExistence(timeout: 20))
                marina.tap()
                XCTAssertTrue(app.buttons["chat-title"].waitForExistence(timeout: 15))
                if frame == "attachment-menu" {
                    let attach = app.buttons["chat.attach"]
                    XCTAssertTrue(attach.waitForExistence(timeout: 10))
                    attach.tap()
                    XCTAssertTrue(app.buttons["Choose files"].waitForExistence(timeout: 10))
                } else {
                    let composer = app.descendants(matching: .any)["composer"].firstMatch
                    XCTAssertTrue(composer.waitForExistence(timeout: 10))
                    composer.tap()
                    XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 10))
                    if frame == "composer-text" { composer.typeText("See you tomorrow") }
                }
            }
            Thread.sleep(forTimeInterval: 2)
            let screenshot = XCTAttachment(screenshot: app.screenshot())
            screenshot.name = "bottom-\(frame)-\(appearance)"
            screenshot.lifetime = .keepAlways
            add(screenshot)
            app.terminate()
        }
        let metadata = XCTAttachment(data: Data(try XCTUnwrap(appliedFixture).utf8), uniformTypeIdentifier: "public.json")
        metadata.name = "bottom-composer-fixture.json"
        metadata.lifetime = .keepAlways
        add(metadata)
    }
}
