import XCTest

/// Destructive actions inside sheets confirm in place (Things-style): the button itself
/// becomes the confirmation. Plan line sheet: [lixeira][Atualizar] → red "Remover item" + ×.
/// Profile sheet: "Bloquear" turns red and asks, × folds it back.
final class SheetConfirmJourneyTests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    @MainActor private func launch(_ open: String) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-RodaDemo", "YES", "-RodaFreshStart", "YES", "-RodaResetDemo", "YES",
                               "-AppleLanguages", "(pt-BR)", "-RodaAppearance", "light",
                               "-RodaApprovalsExplained", "YES", "-RodaOpen", open]
        app.launch()
        return app
    }

    @MainActor
    func testPlanLineUpdateThenRemoveConfirmsInPlace() {
        let app = launch("paraty-plan")
        let escuna = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH 'Escuna'")).firstMatch
        XCTAssertTrue(escuna.waitForExistence(timeout: 20), "the plan shows the schooner line")

        // Update: edit the text, tap Atualizar.
        escuna.tap()
        let update = app.buttons["line-editor-primary"]
        XCTAssertTrue(update.waitForExistence(timeout: 5), "editing a line shows [lixeira][Atualizar]")
        let field = app.textFields["line-editor-text"]
        XCTAssertTrue(field.waitForExistence(timeout: 3))
        field.tap()
        field.typeText(" ao pôr do sol")
        update.tap()
        let renamed = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH 'Escuna' AND label CONTAINS 'pôr do sol'")).firstMatch
        XCTAssertTrue(renamed.waitForExistence(timeout: 5), "Atualizar saved the line")
        sleep(1)

        // Remove: the trash grows into the confirmation; × folds it back.
        renamed.tap()
        let trash = app.buttons["line-editor-destructive"]
        XCTAssertTrue(trash.waitForExistence(timeout: 5))
        trash.tap()
        let confirm = app.buttons["line-editor-confirm"]
        XCTAssertTrue(confirm.waitForExistence(timeout: 3), "the trash became 'Remover item'")
        XCTAssertTrue(app.buttons["line-editor-cancel"].exists, "Atualizar shrank into ×")
        XCTAssertFalse(app.buttons["line-editor-primary"].exists)
        app.buttons["line-editor-cancel"].tap()
        XCTAssertTrue(app.buttons["line-editor-primary"].waitForExistence(timeout: 3), "× brings Atualizar back")
        sleep(1)

        trash.tap()
        XCTAssertTrue(confirm.waitForExistence(timeout: 3))
        confirm.tap()
        XCTAssertTrue(app.buttons["line-editor-primary"].waitForNonExistence(timeout: 5), "confirming closes the sheet")
        XCTAssertTrue(renamed.waitForNonExistence(timeout: 5), "the line is gone")
    }

    @MainActor
    func testProfileBlockAsksInPlace() {
        let app = launch("paraty")
        let names = app.staticTexts.matching(NSPredicate(format: "label == 'Marina'"))
        XCTAssertTrue(names.firstMatch.waitForExistence(timeout: 20))
        sleep(2)
        // Her latest message's name (the first one can sit under the top bar).
        names.allElementsBoundByIndex.filter { $0.isHittable }.last?.tap()
        let block = app.buttons["profile-block"]
        // The danger rows sit at the bottom of the sheet.
        for _ in 0..<4 where !block.isHittable { app.swipeUp() }
        XCTAssertTrue(block.waitForExistence(timeout: 5))
        block.tap()
        XCTAssertTrue(app.buttons["profile-block-confirm"].waitForExistence(timeout: 3), "Bloquear turned into its question")
        sleep(1)
        app.buttons["profile-block-cancel"].tap()
        XCTAssertTrue(app.buttons["profile-block"].waitForExistence(timeout: 3), "× folds it back")
        XCTAssertFalse(app.buttons["profile-block-confirm"].exists)
    }
}
