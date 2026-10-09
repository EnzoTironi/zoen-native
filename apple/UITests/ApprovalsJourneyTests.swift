import XCTest

/// The approvals catch-up stack, driven like a person: swipe or tap the glass buttons,
/// open the details, undo. Showcase seed in pt-BR: Financeiro asks about the Pousada,
/// the bus tickets and sending Marina the bill (Paraty); Zoen asks to read Lúcia's
/// calendar (Turma, a red line) and to install a Slack hook (Zoen · Produto).
final class ApprovalsJourneyTests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    @MainActor
    /// `showcase: false` on a relaunch: the showcase flag reseeds the demo on every launch,
    /// which would wipe what the person just decided.
    private func launch(fresh: Bool = true, open: String = "aprovacoes", showcase: Bool = true) -> XCUIApplication {
        let app = XCUIApplication()
        var args = ["-RodaDemo", "YES", "-AppleLanguages", "(pt-BR)",
                    "-RodaAppearance", "light", "-RodaApprovalsExplained", "YES", "-RodaOpen", open]
        if showcase { args += ["-RodaShowcase", "YES"] }
        if fresh { args += ["-RodaFreshStart", "YES", "-RodaResetDemo", "YES"] }
        app.launchArguments = args
        app.launch()
        return app
    }

    @MainActor private func card(_ app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any)["approval-card"].firstMatch
    }

    /// The top card's text, once it is on screen.
    @MainActor private func topCard(_ app: XCUIApplication, _ file: StaticString = #filePath, _ line: UInt = #line) -> String {
        let c = card(app)
        XCTAssertTrue(c.waitForExistence(timeout: 20), "an approval card is on top", file: file, line: line)
        return c.label
    }

    /// Waits until the top card is a different one (or the stack is done).
    @MainActor private func waitForNext(_ app: XCUIApplication, after label: String) -> Bool {
        let gone = NSPredicate { _, _ in
            let c = app.descendants(matching: .any)["approval-card"].firstMatch
            return !c.exists || c.label != label
        }
        return XCTWaiter().wait(for: [XCTNSPredicateExpectation(predicate: gone, object: nil)], timeout: 6) == .completed
    }

    /// A finger drag on the card: sideways flicks, a long deliberate pull up or down.
    @MainActor private func drag(_ app: XCUIApplication, dx: CGFloat, dy: CGFloat) {
        let start = card(app).coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        start.press(forDuration: 0.05, thenDragTo: start.withOffset(CGVector(dx: dx, dy: dy)),
                    withVelocity: .default, thenHoldForDuration: 0.05)
    }

    @MainActor private func undoToast(_ app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any)["approval-undo"].firstMatch
    }

    /// The undo window closes and the decision is committed to the core.
    @MainActor private func waitForCommit(_ app: XCUIApplication) {
        let gone = NSPredicate(format: "exists == false")
        XCTAssertEqual(XCTWaiter().wait(for: [XCTNSPredicateExpectation(predicate: gone, object: undoToast(app))], timeout: 10),
                       .completed, "the undo toast goes away and the decision lands")
    }

    /// Deny cards (left swipe) until one whose text contains `text` is on top.
    @MainActor private func denyUntil(_ app: XCUIApplication, contains text: String) {
        for _ in 0..<8 {
            let label = topCard(app)
            if label.contains(text) { return }
            app.buttons["approval-deny"].tap()
            XCTAssertTrue(waitForNext(app, after: label))
        }
        XCTFail("never reached a card about \(text)")
    }

    @MainActor
    func testSwipeRightApprovesJustThisOnce() {
        let app = launch()
        let first = topCard(app)
        drag(app, dx: 320, dy: 10)
        XCTAssertTrue(waitForNext(app, after: first), "the card flies off to the right")
        XCTAssertTrue(undoToast(app).waitForExistence(timeout: 3))
        XCTAssertTrue(app.staticTexts["Aprovado"].waitForExistence(timeout: 4), "the toast says what happened")
        waitForCommit(app)

        // Close and come back from the bell: the approved ask doesn't return.
        app.buttons["approvals-back"].tap()
        let bell = app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Notificações'")).firstMatch
        XCTAssertTrue(bell.waitForExistence(timeout: 10))
        bell.tap()
        XCTAssertNotEqual(topCard(app), first, "the approved request is out of the stack")
    }

    @MainActor
    func testSwipeLeftDenies() {
        let app = launch()
        let first = topCard(app)
        drag(app, dx: -320, dy: 10)
        XCTAssertTrue(waitForNext(app, after: first), "the card flies off to the left")
        XCTAssertTrue(undoToast(app).waitForExistence(timeout: 3))
        XCTAssertTrue(app.staticTexts["Negado"].waitForExistence(timeout: 4), "the toast says what happened")
        waitForCommit(app)
        XCTAssertNotEqual(topCard(app), first)
    }

    @MainActor
    func testUndoBringsTheCardBack() {
        let app = launch()
        let first = topCard(app)
        drag(app, dx: 320, dy: 10)
        XCTAssertTrue(waitForNext(app, after: first))
        XCTAssertTrue(undoToast(app).waitForExistence(timeout: 3))
        undoToast(app).tap()
        let back = NSPredicate { _, _ in app.descendants(matching: .any)["approval-card"].firstMatch.label == first }
        XCTAssertEqual(XCTWaiter().wait(for: [XCTNSPredicateExpectation(predicate: back, object: nil)], timeout: 5),
                       .completed, "undo puts the same card back on top")
        // Nothing was decided: the card is still there after the undo window would have closed.
        sleep(6)
        XCTAssertEqual(topCard(app), first)
    }

    @MainActor
    func testAlwaysApproveAlsoClearsTheSameAskFromThatAgent() {
        let app = launch()
        // Financeiro has two "send outside Zoen" asks for Marina in Paraty: the bill and the itinerary.
        denyUntil(app, contains: "Mandar")
        let marina = topCard(app)
        let other = marina.contains("cobrança de R$ 694") ? "roteiro de Paraty" : "cobrança de R$ 694"
        drag(app, dx: 0, dy: -420)
        XCTAssertTrue(waitForNext(app, after: marina), "a long pull up sends it off")
        XCTAssertTrue(undoToast(app).waitForExistence(timeout: 3))
        XCTAssertTrue(app.staticTexts["Sempre aprovado"].waitForExistence(timeout: 4), "the toast says what happened")
        waitForCommit(app)

        // Go through what's left: the twin ask never shows up again.
        for _ in 0..<8 {
            let c = card(app)
            if !c.waitForExistence(timeout: 3) { break }
            let label = c.label
            XCTAssertFalse(label.contains(other), "the matching ask was approved by the standing decision")
            app.buttons["approval-deny"].tap()
            XCTAssertTrue(waitForNext(app, after: label))
        }
        XCTAssertTrue(app.descendants(matching: .any)["approvals-done"].waitForExistence(timeout: 10), "all caught up")
    }

    @MainActor
    func testAlwaysDenyThenRevokeInPermissions() {
        let app = launch()
        denyUntil(app, contains: "Reservar a Pousada")
        let pousada = topCard(app)
        drag(app, dx: 0, dy: 420)
        XCTAssertTrue(waitForNext(app, after: pousada), "a long pull down sends it off")
        XCTAssertTrue(undoToast(app).waitForExistence(timeout: 3))
        XCTAssertTrue(app.staticTexts["Sempre negado"].waitForExistence(timeout: 4), "the toast says what happened")
        waitForCommit(app)
        app.buttons["approvals-back"].tap()

        // Financeiro's permissions in Paraty list the standing decision, and it can be revoked.
        app.terminate()
        let again = launch(fresh: false, open: "permissoes", showcase: false)
        // The standing decisions sit below the tools, so scroll down to them like a person would.
        let row = again.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS 'Sempre negar'")).firstMatch
        let none = again.staticTexts["Nenhuma ainda."]
        XCTAssertTrue(again.navigationBars.firstMatch.waitForExistence(timeout: 20) || again.staticTexts["Ferramentas"].waitForExistence(timeout: 5))
        for _ in 0..<6 where !row.exists && !none.exists {
            again.swipeUp()
        }
        let found = row.waitForExistence(timeout: 3)
        if !found { keep(again, "permissions") }
        XCTAssertFalse(none.exists && !found, "the always-deny was saved (not reset on relaunch)")
        XCTAssertTrue(found, "Permissões shows the standing deny")
        let revoke = again.buttons["revoke-standing"].firstMatch
        XCTAssertTrue(revoke.waitForExistence(timeout: 5))
        // One tap revokes at once; the toast offers Desfazer, which brings it back.
        revoke.tap()
        XCTAssertTrue(none.waitForExistence(timeout: 3), "one tap revokes")
        let undo = again.buttons["toast-undo"]
        XCTAssertTrue(undo.waitForExistence(timeout: 3), "the toast offers Desfazer")
        undo.tap()
        XCTAssertTrue(row.waitForExistence(timeout: 3), "Desfazer brings the decision back")
        sleep(1)
        // Revoke for real and let the undo window pass: it stays gone.
        again.buttons["revoke-standing"].firstMatch.tap()
        XCTAssertTrue(none.waitForExistence(timeout: 3))
        sleep(7)
        XCTAssertTrue(none.exists, "revoked: none left")
        XCTAssertFalse(row.exists, "and the agent will ask again")
    }

    @MainActor
    func testTapForDetailsThenDecideThere() {
        let app = launch()
        let first = topCard(app)
        card(app).tap()
        let details = NSPredicate { _, _ in app.descendants(matching: .any)["approval-card"].firstMatch.label.contains("Quem está pedindo") }
        XCTAssertEqual(XCTWaiter().wait(for: [XCTNSPredicateExpectation(predicate: details, object: nil)], timeout: 8),
                       .completed, "details show who's asking, what it touches and past decisions")
        XCTAssertTrue(card(app).label.contains("Decisões anteriores"))
        // The same glass buttons work from the details.
        app.buttons["approval-deny"].tap()
        XCTAssertTrue(waitForNext(app, after: first))
        XCTAssertTrue(undoToast(app).waitForExistence(timeout: 3))
        XCTAssertFalse(card(app).label.contains("Quem está pedindo"), "the next card opens as a summary")
    }

    /// A screenshot and the element tree, kept in the result bundle for a post-mortem.
    @MainActor
    private func keep(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
        let tree = XCTAttachment(string: app.debugDescription)
        tree.name = "\(name) (tree)"
        tree.lifetime = .keepAlways
        add(tree)
    }
}
