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

    /// Person sheet from a sender name, then Guia via the title → participants list → art picker.
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
        // Title capsule opens participants ("Shows who's in this chat").
        let title = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Viajantes")).firstMatch
        if title.waitForExistence(timeout: 5) {
            title.tap()
        } else {
            app.buttons["zoenBack"].coordinate(withNormalizedOffset: CGVector(dx: 4.5, dy: 0.5)).tap()
        }
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
