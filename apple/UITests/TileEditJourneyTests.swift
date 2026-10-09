import XCTest

/// Jiggle edit mode on the mini-app tiles (Home strip and a chat's pinned tiles), driven like
/// a person: long-press to start, drag a card over its neighbour, minus → confirm to unpin,
/// "Concluir" to finish; then a relaunch keeps the new order and the unpinned card stays gone.
final class TileEditJourneyTests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    /// `-RodaShowcase` re-arranges Home on every launch (stage setting), so only the fresh
    /// launch uses it; a relaunch must show what the person left.
    @MainActor private func launch(fresh: Bool, extra: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        var args = ["-RodaDemo", "YES", "-AppleLanguages", "(pt-BR)",
                    "-RodaAppearance", "light", "-RodaApprovalsExplained", "YES"]
        if fresh { args += ["-RodaShowcase", "YES"] }
        if fresh { args += ["-RodaFreshStart", "YES", "-RodaResetDemo", "YES"] }
        app.launchArguments = args + extra
        app.launch()
        return app
    }

    /// The tiles of a strip, left to right, by their titles.
    @MainActor private func tiles(_ app: XCUIApplication, _ prefix: String) -> [XCUIElement] {
        let q = app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH %@ AND NOT (identifier BEGINSWITH %@) AND identifier != %@",
            "\(prefix)-", "\(prefix)-remove", "\(prefix)-done"))
        let width = app.windows.firstMatch.frame.width
        // Only the strip on screen (another copy of Home can sit off to the side); a card the
        // strip scrolled half out after a drop still counts.
        return q.allElementsBoundByIndex
            .filter { $0.exists && $0.frame.width > 60 && $0.frame.maxX > 8 && $0.frame.minX < width }
            .sorted { $0.frame.minX < $1.frame.minX }
    }
    @MainActor private func names(_ app: XCUIApplication, _ prefix: String) -> [String] {
        tiles(app, prefix).map { String($0.identifier.dropFirst(prefix.count + 1)) }
    }

    @MainActor
    func testHomeTilesJiggleReorderUnpinAndKeepIt() {
        var app = launch(fresh: true)
        let first = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'home-tile-'")).firstMatch
        XCTAssertTrue(first.waitForExistence(timeout: 20), "Home shows mini-app cards")
        sleep(4) // let launch work settle so the hold reads as a hold, not a tap
        let before = names(app, "home-tile")
        XCTAssertGreaterThanOrEqual(before.count, 2, "at least two cards to reorder")

        // Long-press: edit mode, with a minus on every card and "Concluir".
        tiles(app, "home-tile")[0].press(forDuration: 1.2)
        let done = app.buttons["home-tile-done"]
        if !done.waitForExistence(timeout: 4) {
            // A hold that lands while launch work still settles can read as a tap and open
            // the app; close it and hold again.
            let close = app.buttons["miniapp-close"].firstMatch
            if close.waitForExistence(timeout: 3) { close.tap(); sleep(2) }
            tiles(app, "home-tile")[0].press(forDuration: 1.5)
        }
        XCTAssertTrue(done.waitForExistence(timeout: 4), "long-press enters edit mode")
        XCTAssertTrue(app.buttons["home-tile-remove-\(before[0])"].exists, "each card shows a minus")
        sleep(1)

        // Drag the first card over the second: they swap.
        let t0 = tiles(app, "home-tile")[0]
        let start = t0.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.6))
        // Hold long enough to lift, move at finger speed, and rest over the neighbour.
        start.press(forDuration: 0.8, thenDragTo: start.withOffset(CGVector(dx: t0.frame.width + 40, dy: 0)),
                    withVelocity: XCUIGestureVelocity(200), thenHoldForDuration: 0.6)
        sleep(1)
        let moved = names(app, "home-tile")
        XCTAssertEqual(Array(moved.prefix(2)), [before[1], before[0]], "the dragged card took its neighbour's place")

        // Minus on the (now) second card → confirm → it's gone and the rest close the gap.
        let victim = moved[1]
        app.buttons["home-tile-remove-\(victim)"].tap()
        // The dialog's own button (its title repeats the words as text).
        let byId = app.buttons["home-tile-remove-confirm"].firstMatch
        let confirm = byId.waitForExistence(timeout: 3) ? byId
            : app.buttons.matching(NSPredicate(format: "label == 'Tirar do Início'")).firstMatch
        XCTAssertTrue(confirm.exists, "removing asks first")
        confirm.tap()
        // Poll on the main thread (a block predicate's queries can stall) for up to 10s.
        var left = names(app, "home-tile")
        for _ in 0..<10 where left.contains(victim) { sleep(1); left = names(app, "home-tile") }
        XCTAssertFalse(left.contains(victim), "the card is unpinned (still: \(left))")
        sleep(1)

        // Concluir: the wobble and minus badges go away.
        done.tap()
        XCTAssertTrue(app.buttons["home-tile-done"].waitForNonExistence(timeout: 4), "Concluir leaves edit mode")
        XCTAssertFalse(app.buttons["home-tile-remove-\(moved[0])"].exists, "no minus outside edit mode")
        let after = names(app, "home-tile")

        // Relaunch (no fresh start): the order and the unpin are remembered.
        app.terminate()
        app = launch(fresh: false)
        XCTAssertTrue(app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'home-tile-'")).firstMatch.waitForExistence(timeout: 20))
        sleep(2)
        XCTAssertEqual(names(app, "home-tile"), after, "the new order survives a relaunch")
        XCTAssertFalse(names(app, "home-tile").contains(victim), "the unpinned card stays off Home")
    }

    @MainActor
    func testChatPinsJiggleAndReorder() {
        let app = launch(fresh: true, extra: ["-RodaOpen", "turma", "-RodaChatScrollTop", "YES"])
        let first = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'pin-tile-'")).firstMatch
        XCTAssertTrue(first.waitForExistence(timeout: 20), "the chat shows its pinned tiles")
        sleep(3)
        let before = names(app, "pin-tile")
        XCTAssertGreaterThanOrEqual(before.count, 2)
        tiles(app, "pin-tile")[0].press(forDuration: 0.9)
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "pin-press"; shot.lifetime = .keepAlways; add(shot)
        XCTAssertTrue(app.buttons["pin-tile-done"].waitForExistence(timeout: 4), "long-press enters edit mode in the chat too")
        sleep(1)
        let t0 = tiles(app, "pin-tile")[0]
        let start = t0.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.6))
        start.press(forDuration: 0.4, thenDragTo: start.withOffset(CGVector(dx: t0.frame.width + 30, dy: 0)),
                    withVelocity: XCUIGestureVelocity(300), thenHoldForDuration: 0.3)
        sleep(1)
        XCTAssertEqual(Array(names(app, "pin-tile").prefix(2)), [before[1], before[0]], "pins reorder by dragging")
        app.buttons["pin-tile-done"].tap()
        XCTAssertTrue(app.buttons["pin-tile-done"].waitForNonExistence(timeout: 4))
        // Outside edit mode a tap opens the mini-app as before.
        tiles(app, "pin-tile")[0].tap()
        XCTAssertTrue(app.descendants(matching: .any)["miniapp-sheet"].waitForExistence(timeout: 8) || app.buttons["Fechar"].waitForExistence(timeout: 2),
                      "a tap still opens the mini-app")
    }
}
