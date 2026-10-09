import XCTest

/// Paced flows for screen-recorded proof videos (`roda-shots/record-test.sh`).
/// Showcase seed, pt-BR, light. Each test asserts its key moment so a broken video fails.
final class ProofVideoTests: XCTestCase {
    @MainActor
    private func launch(_ extra: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = [
            "-RodaShowcase", "YES", "-RodaDemo", "YES", "-RodaFreshStart", "YES", "-RodaResetDemo", "YES",
            "-RodaWowReset", "YES", "-AppleLanguages", "(pt-BR)", "-RodaAppearance", "light",
        ] + extra
        app.launch()
        return app
    }

    /// Approvals stack: slow drags (overlay ramp, threshold), a cancelled drag springing
    /// back, a fast flick, details then a long pull up ("Sempre aprovar"), a pull down
    /// ("Sempre negar"), a button, and the all-caught-up end state.
    @MainActor
    func testApprovalsSwipeProof() throws {
        let app = launch(["-RodaApprovalsExplained", "YES", "-RodaOpen", "aprovacoes"])
        let card = app.descendants(matching: .any)["approval-card"].firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 20), "the stack opens on a card")
        sleep(2)
        func grab() -> XCUICoordinate { card.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.32)) }
        func pull(_ dx: CGFloat, _ dy: CGFloat, speed: CGFloat, hold: TimeInterval = 0.5) {
            let g = grab()
            g.press(forDuration: 0.15, thenDragTo: g.withOffset(CGVector(dx: dx, dy: dy)),
                    withVelocity: XCUIGestureVelocity(speed), thenHoldForDuration: hold)
        }
        func next(after label: String) {
            let moved = NSPredicate { _, _ in !card.exists || card.label != label }
            XCTAssertEqual(XCTWaiter().wait(for: [XCTNSPredicateExpectation(predicate: moved, object: nil)], timeout: 6), .completed)
        }

        // 1. Slow drag right past the line: the green wash and "Aprovar" ramp in.
        var label = card.label
        pull(230, 24, speed: 220)
        next(after: label)
        sleep(1)
        // 2. A hesitant drag left, let go short of the line: it springs back.
        label = card.label
        pull(-90, 10, speed: 160, hold: 0.4)
        sleep(1)
        XCTAssertEqual(card.label, label, "a short drag doesn't decide")
        // 3. A fast flick left: it flies off the way the finger went.
        pull(-120, -30, speed: 2600, hold: 0)
        next(after: label)
        sleep(1)
        // 4. Tap for details, then a long pull up: the ink "SEMPRE" stamp.
        card.tap()
        // Waits for the flip rather than a fixed nap: a cold first launch can stall a beat.
        let flipped = NSPredicate { _, _ in card.label.contains("Quem está pedindo") }
        XCTAssertEqual(XCTWaiter().wait(for: [XCTNSPredicateExpectation(predicate: flipped, object: nil)], timeout: 8), .completed, "details open")
        sleep(1)
        label = card.label
        pull(10, -420, speed: 260, hold: 0.3)
        next(after: label)
        sleep(2)
        // 5. A long pull down: "Sempre negar".
        label = card.label
        pull(-6, 420, speed: 260, hold: 0.3)
        next(after: label)
        sleep(2)
        // 6. The glass button, then the end state.
        label = card.label
        app.buttons["approval-approve"].tap()
        next(after: label)
        XCTAssertTrue(app.descendants(matching: .any)["approvals-done"].waitForExistence(timeout: 8), "all caught up")
        sleep(4)
    }

    /// WOW: Store ink stamp → new Space draws in its art → first message flourish.
    @MainActor
    func testWowMomentsProof() throws {
        let app = launch(["-RodaTab", "store"])
        let get = app.buttons["Obter"].firstMatch
        XCTAssertTrue(get.waitForExistence(timeout: 20), "Store has a Get button")
        sleep(2)
        get.tap()
        sleep(3)

        app.buttons["Espaços"].firstMatch.tap()
        let create = app.buttons["createSpace"].exists ? app.buttons["createSpace"] : app.buttons["createSpaceEmpty"]
        XCTAssertTrue(create.waitForExistence(timeout: 8))
        create.tap()
        let name = app.textFields["spaceNameField"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        name.tap()
        name.typeText("Praia no feriado")
        sleep(1)
        app.buttons["confirmCreateSpace"].tap()
        sleep(4)

        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10), "opened the new Space")
        composer.tap()
        composer.typeText("Bora? Saída sexta 18h 🌊")
        app.buttons["send"].firstMatch.tap()
        sleep(4)
    }

    /// Person sheet from a sender name, then Guia via the title menu → Membros → art picker.
    @MainActor
    func testProfileSheetProof() throws {
        let app = launch(["-RodaOpen", "coastal"])
        XCTAssertTrue(app.buttons["zoenBack"].waitForExistence(timeout: 20))
        sleep(3)
        let ana = app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", "Ana")).firstMatch
        XCTAssertTrue(ana.waitForExistence(timeout: 12), "Ana name visible")
        ana.tap()
        sleep(3)
        if app.buttons["OK"].firstMatch.exists { app.buttons["OK"].firstMatch.tap() }
        else { app.swipeDown(velocity: .slow) }
        sleep(2)
        // The title grows into the chat menu; Membros lists who's here.
        let title = app.buttons["chat-title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        title.tap()
        let members = app.buttons["header-menu-members"]
        XCTAssertTrue(members.waitForExistence(timeout: 3), "the header menu opens")
        sleep(1)
        members.tap()
        sleep(2)
        let guia = app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", "Guia")).firstMatch
        XCTAssertTrue(guia.waitForExistence(timeout: 10), "Guia in participants")
        guia.tap()
        sleep(4)
        let choose = app.buttons["Escolher desenho"].firstMatch
        XCTAssertTrue(choose.waitForExistence(timeout: 8), "agent profile offers Escolher desenho")
        choose.tap()
        sleep(2)
        let arts = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'art-'"))
        XCTAssertTrue(arts.firstMatch.waitForExistence(timeout: 5), "picker shows drawings")
        arts.element(boundBy: min(4, max(0, arts.count - 1))).tap()
        sleep(2)
        if app.buttons["OK"].firstMatch.exists { app.buttons["OK"].firstMatch.tap() }
        sleep(3)
    }

    /// Hold + → record → release: the voice note flies into Zoen's chat.
    @MainActor
    func testPlusVoiceProof() throws {
        let app = launch(["-RodaVoiceDemo", "plus"])
        XCTAssertTrue(app.staticTexts["Turma do Sábado"].waitForExistence(timeout: 20))
        sleep(1)
        let plus = app.buttons["Criar e mais"]
        XCTAssertTrue(plus.waitForExistence(timeout: 12), "radial + visible on home (pt-BR)")
        sleep(1)
        // Long hold so the recording row shows; release sends and opens Zoen's chat.
        plus.press(forDuration: 2.6)
        let voice = app.descendants(matching: .any)["voice-message"]
        XCTAssertTrue(voice.waitForExistence(timeout: 14), "voice message visible in Zoen chat")
        sleep(3)
    }
}
