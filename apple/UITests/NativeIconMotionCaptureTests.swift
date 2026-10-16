import XCTest
import UIKit

/// Diagnostic real-input producer. Attachments are deliberately NOT the completed
/// 40-state manifest: native recording must establish aligned phases first.
/// Run on the reserved device with -parallel-testing-enabled NO. Record its screen
/// externally for 5–10 seconds; do not disable XCTest idleness to invent timing.
final class NativeIconMotionCaptureTests: XCTestCase {
    @MainActor func testLightActionSequence() throws { try sequence(appearance: "light") }
    @MainActor func testDarkActionSequence() throws { try sequence(appearance: "dark") }
    @MainActor func testLightSearchFilterJourney() throws { try searchJourney(appearance: "light") }
    @MainActor func testDarkSearchFilterJourney() throws { try searchJourney(appearance: "dark") }

    @MainActor private func searchJourney(appearance: String) throws {
        let env = ProcessInfo.processInfo.environment
        guard env["ZOEN_PARITY_FIXTURE"] == "native-parity-v1" else { throw XCTSkip("Requires native parity fixture") }
        let app = XCUIApplication()
        app.launchEnvironment["ZOEN_PARITY_FIXTURE"] = "native-parity-v1"
        app.launchEnvironment["ZOEN_PARITY_REVISION"] = try XCTUnwrap(env["ZOEN_PARITY_REVISION"])
        app.launchEnvironment["TZ"] = "UTC"
        app.launchArguments = ["-RodaFreshStart", "YES", "-RodaResetDemo", "YES", "-RodaDemo", "YES",
            "-AppleLanguages", "(en)", "-AppleLocale", "en_US", "-RodaAppearance", appearance]
        app.launch()
        let search = app.textFields["chat-search-field"]
        XCTAssertTrue(search.waitForExistence(timeout: 40))
        XCTAssertEqual(app.navigationBars.count, 0)
        let filter = app.buttons["chat-filter"]
        XCTAssertTrue(filter.exists)
        XCTAssertEqual(filter.value as? String, "All chats")
        XCTAssertLessThan(abs(search.frame.midY - filter.frame.midY), 4, "Search and scope share one surface")
        let tile = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "home-tile-")).firstMatch
        if tile.exists { XCTAssertLessThan(search.frame.maxY, tile.frame.minY) }
        attach(app.screenshot(), name: "search-glass-top-\(appearance)")
        search.tap()
        search.typeText("Marina")
        filter.tap()
        let direct = app.buttons["Direct chats"].firstMatch
        XCTAssertTrue(direct.waitForExistence(timeout: 5))
        direct.tap()
        XCTAssertEqual(filter.value as? String, "Direct chats")
        let marina = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Marina")).firstMatch
        XCTAssertTrue(marina.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Paraty with Marina")).firstMatch.exists)
        attach(app.screenshot(), name: "search-glass-filtered-\(appearance)")
        app.buttons["Clear search"].tap()
        XCTAssertEqual(search.value as? String, "Search chats")
        filter.tap()
        app.buttons["All chats"].firstMatch.tap()
        app.keyboards.buttons["Search"].firstMatch.tap()
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Paraty with Marina")).firstMatch.waitForExistence(timeout: 5))
        XCTAssertEqual(filter.value as? String, "All chats")
        attach(app.screenshot(), name: "search-glass-reset-\(appearance)")
    }

    @MainActor private func sequence(appearance: String) throws {
        let env = ProcessInfo.processInfo.environment
        guard env["ZOEN_PARITY_FIXTURE"] == "native-parity-v1" else {
            throw XCTSkip("Requires explicit native parity fixture")
        }
        let revision = try XCTUnwrap(env["ZOEN_PARITY_REVISION"])
        XCTAssertNotNil(revision.range(of: "^[0-9a-f]{40}$", options: .regularExpression))
        continueAfterFailure = false
        XCUIDevice.shared.orientation = .portrait
        let app = XCUIApplication()
        app.launchEnvironment["ZOEN_PARITY_FIXTURE"] = "native-parity-v1"
        app.launchEnvironment["ZOEN_PARITY_REVISION"] = revision
        app.launchEnvironment["TZ"] = "UTC"
        app.launchEnvironment["ZOEN_PARITY_MOTION_DRIVER"] = "1"
        app.launchArguments = ["-RodaFreshStart", "YES", "-RodaResetDemo", "YES",
            "-RodaDemo", "YES", "-AppleLanguages", "(en)", "-AppleLocale", "en_US",
            "-RodaAppearance", appearance]
        app.launch()
        let fixture = app.descendants(matching: .any)["parity-applied-fixture"].firstMatch
        XCTAssertTrue(fixture.waitForExistence(timeout: 40))
        let applied = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(fixture.label.utf8)) as? [String: Any])
        XCTAssertEqual(applied["source_revision"] as? String, revision)
        XCTAssertTrue(app.buttons["New message"].firstMatch.waitForExistence(timeout: 20))
        XCTAssertEqual(app.navigationBars.count, 0, "Root navigation has no top chrome")
        for title in ["Chats", "Spaces", "Notifications", "Profile", "New message"] {
            XCTAssertGreaterThan(app.buttons[title].firstMatch.frame.midY, app.frame.midY,
                                 "Every root destination/action belongs in the bottom bar")
        }
        var actions: [[String: Any]] = []
        let state = app.descendants(matching: .any)["parity-motion-state"].firstMatch
        XCTAssertTrue(state.waitForExistence(timeout: 10))
        func pendingWitness() throws -> [String: Any] {
            let json = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(state.label.utf8)) as? [String: Any])
            XCTAssertEqual(json["driver_error"] as? String, "")
            return try XCTUnwrap(json["notifications"] as? [String: Any])
        }
        func assertEffect(_ expected: String, after previous: Int) {
            let submitted = NSPredicate { _, _ in
                guard let json = try? JSONSerialization.jsonObject(with: Data(state.label.utf8)) as? [String: Any] else { return false }
                if json["reduce_motion"] as? Bool == true {
                    return (json["native_effect_event"] as? Int ?? 0) == previous
                }
                return json["native_effect_diagnostic"] as? String == expected
                    && (json["native_effect_event"] as? Int ?? 0) > previous
            }
            expectation(for: submitted, evaluatedWith: state)
            waitForExpectations(timeout: 10)
        }
        func effectEvent() -> Int {
            let json = (try? JSONSerialization.jsonObject(with: Data(state.label.utf8))) as? [String: Any]
            return json?["native_effect_event"] as? Int ?? 0
        }
        let baseline = try pendingWitness()
        let initialCount = try XCTUnwrap(baseline["pendingCount"] as? Int)
        XCTAssertEqual(initialCount, 3)
        let beforeBell = effectEvent()
        app.buttons["parity-open-genuine-request"].tap()
        let increased = NSPredicate { _, _ in
            guard let json = try? JSONSerialization.jsonObject(with: Data(state.label.utf8)) as? [String: Any],
                  let notifications = json["notifications"] as? [String: Any],
                  let count = notifications["pendingCount"] as? Int else { return false }
            return count == initialCount + 1
        }
        expectation(for: increased, evaluatedWith: state)
        waitForExpectations(timeout: 10)
        assertEffect("bell-wiggle-submitted-to-symbol-image", after: beforeBell)
        actions.append(["event": "bell-new", "before": baseline, "after": try pendingWitness(),
                        "native_driver_witness": state.label])
        try attachJSON(try XCTUnwrap(actions.last), name: "bell-new-witness-\(appearance)")
        attach(app.screenshot(), name: "bell-new-\(appearance)")

        // Chats starts selected: first visit another tab so both selection-changing
        // and repeat-selected Chats taps are real UI events.
        app.tabBars.buttons["Spaces"].firstMatch.tap()
        for (label, semantic) in [("Chats", "chats"), ("Spaces", "spaces"),
                                  ("Notifications", "notifications"), ("Profile", "profile")] {
            let button = app.tabBars.buttons[label].firstMatch
            XCTAssertTrue(button.waitForExistence(timeout: 10))
            for repetition in ["first", "repeat"] {
                let start = ProcessInfo.processInfo.systemUptime
                let beforeTap = effectEvent()
                button.tap()
                assertEffect(label == "Profile" ? "Profile-bitmap-feedback-submitted" : "\(label)-bounce-submitted-to-symbol-image", after: beforeTap)
                XCTAssertTrue(button.isSelected, "Selection witness must follow the actual tab handler")
                let shot = app.screenshot()
                let elapsed = (ProcessInfo.processInfo.systemUptime - start) * 1000
                let name = "tab-\(semantic)-tap-\(repetition)-\(appearance)"
                attach(shot, name: name)
                actions.append(["event": "tab-\(semantic)-tap", "repetition": repetition,
                    "dispatch_uptime_seconds": start, "capture_completed_after_ms": elapsed,
                    "observed_state": ["tab": semantic], "selected": button.isSelected,
                    "native_driver_witness": state.label])
                try attachJSON(try XCTUnwrap(actions.last), name: "\(name)-witness")
            }
        }
        app.tabBars.buttons["Chats"].firstMatch.tap()
        let marina = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Marina")).firstMatch
        XCTAssertTrue(marina.waitForExistence(timeout: 15))
        marina.tap()
        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 15))
        XCTAssertTrue(app.buttons["Record voice message"].firstMatch.waitForExistence(timeout: 10))
        composer.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 10))
        let start = ProcessInfo.processInfo.systemUptime
        composer.typeText("See you tomorrow")
        let send = app.buttons["send"].firstMatch
        XCTAssertTrue(send.waitForExistence(timeout: 10))
        XCTAssertTrue(send.isEnabled)
        XCTAssertEqual(composer.value as? String, "See you tomorrow")
        attach(app.screenshot(), name: "composer-send-\(appearance)")
        actions.append(["event": "composer-send", "dispatch_uptime_seconds": start,
            "capture_completed_after_ms": (ProcessInfo.processInfo.systemUptime - start) * 1000,
            "observed_state": ["tab": "chats", "draft": ["text": "See you tomorrow", "focused": true]]])
        // Clear through real keyboard input: do not mutate the draft or an effect counter.
        let clearStart = ProcessInfo.processInfo.systemUptime
        composer.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: "See you tomorrow".count))
        XCTAssertTrue(app.buttons["Record voice message"].firstMatch.waitForExistence(timeout: 10))
        XCTAssertEqual(composer.value as? String, "")
        // Re-query by semantic label: XCTest may retain the stable action element
        // across SwiftUI's send-to-mic replacement.
        let staleSend = app.buttons.matching(NSPredicate(format: "identifier == %@ AND label == %@", "send", "Send")).firstMatch
        // SwiftUI keeps the outgoing send surface alive during its native replacement
        // transition. Assert its removal after settling, not during that transition.
        expectation(for: NSPredicate(format: "exists == false"), evaluatedWith: staleSend)
        waitForExpectations(timeout: 5)
        attach(app.screenshot(), name: "composer-mic-\(appearance)")
        actions.append(["event": "composer-mic", "dispatch_uptime_seconds": clearStart,
            "capture_completed_after_ms": (ProcessInfo.processInfo.systemUptime - clearStart) * 1000,
            "observed_state": ["tab": "chats", "draft": ["text": "", "focused": true]]])
        let report: [String: Any] = ["fixture": applied, "appearance": appearance,
            "runner_reduce_motion": UIAccessibility.isReduceMotionEnabled,
            "status": "diagnostic-partial-not-aligned-icon-manifest", "actions": actions,
            "timing_note": "XCTest tap/typeText await idleness; completion durations are not animation keyframe phases.",
            "pending": ["live typing/connecting peer evidence", "aligned video keyframes", "draw-on phase alignment"]]
        let attachment = XCTAttachment(data: try JSONSerialization.data(withJSONObject: report, options: [.sortedKeys]), uniformTypeIdentifier: "public.json")
        attachment.name = "native-icon-action-sequence-\(appearance).json"
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    @MainActor private func attachJSON(_ value: [String: Any], name: String) throws {
        let attachment = XCTAttachment(data: try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]), uniformTypeIdentifier: "public.json")
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    @MainActor private func attach(_ screenshot: XCUIScreenshot, name: String) {
        let attachment = XCTAttachment(screenshot: screenshot)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
