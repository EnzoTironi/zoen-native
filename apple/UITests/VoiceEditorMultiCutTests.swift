import XCTest

/// The voice editor takes any number of separate cuts (word strikes, waveform selections,
/// filler suggestions), restores one by tapping it, keeps undo/redo across all of them, and
/// the render joins only the kept ranges.
final class VoiceEditorMultiCutTests: XCTestCase {
    private func cuts(_ app: XCUIApplication) -> Int {
        let label = app.descendants(matching: .any)["cut-summary"].label
        // "5 cuts · −1.9s Tap a cut to restore it" → 5
        return Int(label.split(separator: " ").first ?? "") ?? 0
    }

    @MainActor
    func testThreeOrMoreSeparateCutsRenderOnlyTheKeptAudio() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-RodaResetDemo", "YES", "-AppleLanguages", "(en)", "-AppleLocale", "en_US",
                               "-RodaStory", "hike-ride", "-RodaAppAITimeout", "0",
                               "-RodaVoiceEditor", "edit-clean", "-RodaVoiceEditorTest", "YES"]
        app.launch()
        let any = app.descendants(matching: .any)
        XCTAssertTrue(any["word-0"].waitForExistence(timeout: 25), "editor didn't open")
        XCTAssertEqual(cuts(app), 0)

        // 1) strike "So", 2) strike "snacks"
        any["word-0"].tap()
        any["word-12"].tap()
        XCTAssertEqual(cuts(app), 2)

        // 3) select a stretch of waveform near the end and cut it; the selection resets.
        let wave = any["waveform"]
        wave.coordinate(withNormalizedOffset: CGVector(dx: 0.80, dy: 0.5))
            .press(forDuration: 0.15, thenDragTo: wave.coordinate(withNormalizedOffset: CGVector(dx: 0.90, dy: 0.5)))
        XCTAssertTrue(any["cut-selection"].waitForExistence(timeout: 3))
        any["cut-selection"].tap()
        XCTAssertFalse(any["cut-selection"].waitForExistence(timeout: 1), "selection should reset after a cut")
        XCTAssertEqual(cuts(app), 3)

        // 4–5) take the filler suggestion ("um", "uh").
        any["remove-fillers"].tap()
        XCTAssertEqual(cuts(app), 5)
        attach(app, "five-cuts")

        // Tapping a removed word restores just it; undo/redo work across all cuts.
        any["word-12"].tap()
        XCTAssertEqual(cuts(app), 4)
        app.buttons["Undo"].tap()
        XCTAssertEqual(cuts(app), 5)
        app.buttons["Redo"].tap()
        XCTAssertEqual(cuts(app), 4)

        // Render: the real exported file's duration matches the kept ranges.
        any["voice-send"].tap()
        let result = any["render-result"]
        XCTAssertTrue(result.waitForExistence(timeout: 20), "no render result")
        let label = result.label
        attach(app, "rendered")
        func value(_ key: String) -> Double {
            guard let r = label.range(of: "\(key)=") else { return -1 }
            return Double(label[r.upperBound...].prefix { "0123456789.".contains($0) }) ?? -1
        }
        let file = value("file"), expected = value("expected")
        XCTAssertGreaterThan(file, 3.0, label)
        XCTAssertLessThan(file, 6.5, label)
        XCTAssertEqual(file, expected, accuracy: 0.15, label)
        let text = String(label[label.range(of: "text=")!.upperBound...])
        XCTAssertFalse(text.hasPrefix("So"), text)
        XCTAssertFalse(text.contains("um"), text)
        XCTAssertFalse(text.contains("uh"), text)
        XCTAssertTrue(text.contains("snacks"), "restored word must come back: \(text)")
        XCTAssertTrue(text.contains("thermos"), text)
    }

    private func attach(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }
}
