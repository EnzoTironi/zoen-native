import XCTest

/// Ajustes as a person uses it: open it from Você, change the theme, come back one screen at
/// a time, search for something that isn't there, and close it from deep inside.
final class SettingsJourneyTests: XCTestCase {
    private func launch(_ extra: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = [
            "-RodaFreshStart", "YES", "-RodaAccount", "Teste Ajustes:teste_ajustes",
            "-RodaTab", "you", "-AppleLanguages", "(pt-BR)", "-AppleLocale", "pt_BR",
        ] + extra
        app.launch()
        return app
    }

    @MainActor
    func testChangeThemeThenBackPopsOneScreenThenClose() throws {
        continueAfterFailure = false
        let app = launch()
        let open = app.buttons["openSettings"]
        XCTAssertTrue(open.waitForExistence(timeout: 15), "Ajustes is reachable from Você")
        open.tap()

        app.buttons["settings.preferences"].firstMatch.tap()
        XCTAssertTrue(app.buttons["prefsLanguage"].waitForExistence(timeout: 5), "Language opens directly")
        app.buttons["settingsBack"].tap()
        let theme = app.buttons["settings.theme"].firstMatch
        XCTAssertTrue(theme.waitForExistence(timeout: 5), "Theme is reachable from Settings")
        theme.tap()
        let dark = app.buttons["theme.dark"]
        XCTAssertTrue(dark.waitForExistence(timeout: 5), "Tema lists its choices")
        dark.tap()
        XCTAssertTrue(dark.isSelected, "the picked theme is marked")

        // Back pops exactly one: Tema → Ajustes, keeping the settings sheet open.
        app.buttons["settingsBack"].tap()
        XCTAssertTrue(app.textFields["settingsSearch"].waitForExistence(timeout: 3), "back on Ajustes")

        // Search with no match says so, in plain words.
        let search = app.textFields["settingsSearch"]
        search.tap()
        search.typeText("xyzzy")
        XCTAssertTrue(app.descendants(matching: .any)["settingsNoResults"].waitForExistence(timeout: 3), "no-results state")

        // Search that matches jumps straight there.
        app.buttons["Limpar busca"].tap()
        search.typeText("senha")
        XCTAssertTrue(app.buttons["settings.security"].waitForExistence(timeout: 3), "“senha” finds Segurança")
        app.buttons["settings.security"].tap()

        // The X closes Ajustes from anywhere.
        app.buttons["settingsClose"].tap()
        XCTAssertTrue(open.waitForExistence(timeout: 5), "closed back to Você")

        // Leave the theme as we found it.
        open.tap()
        app.buttons["settings.theme"].tap()
        app.buttons["theme.auto"].tap()
    }

    @MainActor
    func testBackupWithoutCopyCannotRestore() throws {
        continueAfterFailure = false
        let app = launch(["-RodaSettings", "backup"])
        let restore = app.descendants(matching: .any)["backupRestore"]
        XCTAssertTrue(restore.waitForExistence(timeout: 15), "Cópia e recuperação opens")
        XCTAssertTrue(app.staticTexts["Ainda não disponível neste aparelho"].exists, "restore says it isn't available yet")
        restore.tap()
        XCTAssertFalse(app.secureTextFields["backupPassword"].waitForExistence(timeout: 1), "tapping restore does nothing")
    }

    @MainActor
    func testDisappearingMessagesShowsEveryOptionDisabled() throws {
        continueAfterFailure = false
        let app = launch(["-RodaSettings", "disappearing"])
        for id in ["disappear.off", "disappear.24h", "disappear.7d", "disappear.90d"] {
            let option = app.buttons[id]
            XCTAssertTrue(option.waitForExistence(timeout: 15), "\(id) is listed")
            XCTAssertFalse(option.isEnabled, "\(id) is disabled while the feature is unavailable")
            XCTAssertFalse(option.isSelected, "no option pretends to be chosen")
        }
    }

    @MainActor
    func testProfileShowsTheRealAccount() throws {
        continueAfterFailure = false
        let app = launch(["-RodaSettings", "x"])
        XCTAssertTrue(app.staticTexts["@teste_ajustes"].waitForExistence(timeout: 15), "the account's own @, nothing else")
        XCTAssertFalse(app.staticTexts["Nenhuma conta neste aparelho"].exists)
    }

    @MainActor
    func testInvitesExplainsTheRewardAndShares() throws {
        continueAfterFailure = false
        let app = launch(["-RodaSettings", "invites"])
        XCTAssertTrue(app.staticTexts["1 mês de Business a cada 5 amigos"].waitForExistence(timeout: 15), "the rule is stated")
        let share = app.buttons["inviteShare"]
        XCTAssertTrue(share.waitForExistence(timeout: 5), "the core gave a link to share")
        if share.exists {
            share.tap()
            XCTAssertTrue(app.otherElements["ActivityListView"].waitForExistence(timeout: 5)
                          || app.navigationBars["UIActivityContentView"].waitForExistence(timeout: 2), "share sheet opens with the link")
        }
    }
}
