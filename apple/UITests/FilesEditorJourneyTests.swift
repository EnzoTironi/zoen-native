import XCTest

/// Pages and files as a person uses them (ADR 0027), paced for the proof video.
/// pt-BR, light. Each step asserts what the person sees.
final class FilesEditorJourneyTests: XCTestCase {
    @MainActor
    private func launch(_ open: String) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = [
            "-RodaFreshStart", "YES", "-RodaDemo", "YES", "-RodaResetDemo", "YES",
            "-AppleLanguages", "(pt-BR)", "-AppleLocale", "pt_BR", "-RodaAppearance", "light",
            "-RodaOpen", open,
        ]
        app.launch()
        return app
    }

    @MainActor
    private func shot(_ app: XCUIApplication, _ name: String) {
        let a = XCTAttachment(screenshot: app.screenshot())
        a.name = name
        a.lifetime = .keepAlways
        add(a)
    }

    /// The header reads "v<n> · você · agora" once version n is saved.
    @MainActor
    private func saved(_ app: XCUIApplication, version: Int) -> Bool {
        app.staticTexts.containing(NSPredicate(format: "label BEGINSWITH %@", "v\(version) ")).firstMatch.waitForExistence(timeout: 15)
    }

    /// Write a page from nothing with Markdown shortcuts, the block menu and the format bar.
    @MainActor
    func testWriteAPage() throws {
        continueAfterFailure = false
        let app = launch("new-page")
        let editor = app.textViews["page.editor"]
        XCTAssertTrue(editor.waitForExistence(timeout: 20), "a new page opens in the editor")
        sleep(1)
        if !app.keyboards.firstMatch.exists { editor.tap() }
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5), "the keyboard is up, ready for the title")
        editor.typeText("Fim de semana em Paraty\n")
        editor.typeText("Três dias de barco e trilha.\n")
        // "[] " becomes a checklist item; return keeps the list going; return on an empty item ends it.
        editor.typeText("[] Reservar a pousada\nAlugar o barco\nComprar protetor\n\n")
        sleep(1)
        shot(app, "01-checklist")
        // "/" opens the block menu above the keyboard.
        editor.typeText("/cit")
        let quote = app.buttons["slash.quote"]
        XCTAssertTrue(quote.waitForExistence(timeout: 5), "the block menu offers Citação")
        shot(app, "02-block-menu")
        quote.tap()
        editor.typeText("Leve dinheiro: muitos lugares não aceitam cartão.\n")
        // "## " makes a heading.
        editor.typeText("## Saída\n")
        editor.typeText("Sexta às ")
        // Bold from the format bar, then typing.
        let bold = app.buttons["Negrito"]
        XCTAssertTrue(bold.waitForExistence(timeout: 5), "the format bar sits above the keyboard")
        bold.tap()
        editor.typeText("7h")
        bold.tap()
        editor.typeText(" no cais.")
        sleep(1)
        shot(app, "03-formatted")
        let text = editor.value as? String ?? ""
        XCTAssertTrue(text.contains("Reservar a pousada"), text)
        XCTAssertTrue(text.contains("Leve dinheiro"), text)
        XCTAssertFalse(text.contains("[]"), "the shortcut turned into a checkbox: \(text)")
        XCTAssertFalse(text.contains("## "), "the shortcut turned into a heading: \(text)")
        // Pausing saves a version for everyone.
        XCTAssertTrue(saved(app, version: 2), "pausing saved version 2")
        shot(app, "04-saved")
        sleep(2)
    }

    /// An imported Markdown page: tick a box, see the versions, go back to the first one.
    @MainActor
    func testVersionsOfAPage() throws {
        continueAfterFailure = false
        let app = launch("page-sample")
        let editor = app.textViews["page.editor"]
        XCTAssertTrue(editor.waitForExistence(timeout: 20), "the imported page opens")
        sleep(2)
        shot(app, "05-imported-page")
        let text = editor.value as? String ?? ""
        XCTAssertTrue(text.contains("Roteiro: Paraty") && text.contains("Alugar o barco"), text)
        XCTAssertFalse(text.contains("- [ ]"), "checklists show as checkboxes, not Markdown: \(text)")
        // Add a line at the end (tap below the text) and pause: a second version.
        editor.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.7)).tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5), "tapping below the text starts writing")
        editor.typeText("\nLevar capa de chuva.")
        XCTAssertTrue(saved(app, version: 2), "pausing saved version 2")
        XCTAssertTrue((editor.value as? String ?? "").contains("Levar capa de chuva."))
        sleep(1)
        app.buttons["page.versions"].tap()
        let v1 = app.descendants(matching: .any)["version.1"]
        XCTAssertTrue(v1.waitForExistence(timeout: 5), "Versões lists the first version")
        sleep(1)
        shot(app, "06-versoes")
        v1.tap()
        let restore = app.buttons["version.restore"]
        XCTAssertTrue(restore.waitForExistence(timeout: 5))
        sleep(2)
        shot(app, "07-version-preview")
        restore.tap()
        sleep(2)
        XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "v3")).firstMatch.waitForExistence(timeout: 6)
                      || app.descendants(matching: .any).containing(NSPredicate(format: "label CONTAINS %@", "versão 3")).firstMatch.exists,
                      "restoring made version 3")
        shot(app, "08-restored")
        sleep(2)
    }

    /// Any file opens with Quick Look.
    @MainActor
    func testOpenAFile() throws {
        continueAfterFailure = false
        let app = launch("file-sample")
        XCTAssertTrue(app.buttons["file.markup"].waitForExistence(timeout: 20), "the file opens with Markup available")
        sleep(3)
        shot(app, "09-file-quicklook")
    }
}
