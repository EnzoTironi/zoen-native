import XCTest

final class InvestorShowcaseTests: XCTestCase {
    @MainActor
    func testCaptureInvestorLightPT() throws {
        capture(appearance: "light")
    }

    @MainActor
    func testCaptureInvestorDarkPT() throws {
        capture(appearance: "dark")
    }

    @MainActor
    private func capture(appearance: String) {
        continueAfterFailure = true
        let lang = "pt-BR"
        // Home
        shoot(appearance: appearance, lang: lang, name: "home", extra: [])
        // Turma — open + scroll to top so pinned tiles show
        shoot(appearance: appearance, lang: lang, name: "chat-turma", extra: [
            "-Open", "turma", "-ChatScrollTop", "YES",
        ])
        // Coastal
        shoot(appearance: appearance, lang: lang, name: "chat-coastal", extra: [
            "-Open", "coastal", "-ChatScrollTop", "YES",
        ])
        // Store
        shoot(appearance: appearance, lang: lang, name: "store", extra: ["-Tab", "store"])
    }

    @MainActor
    private func shoot(appearance: String, lang: String, name: String, extra: [String]) {
        let app = XCUIApplication()
        app.launchArguments = [
            "-Showcase", "YES", "-Demo", "YES", "-FreshStart", "YES",
            "-ResetDemo", "YES",
            "-AppleLanguages", "(\(lang))",
        ] + extra
        // App-level override; `-AppleInterfaceStyle` does not reach the simulator app.
        app.launchArguments += ["-Appearance", appearance]
        app.launch()
        // Settle chrome + optional scroll-top task
        sleep(name.hasPrefix("chat") ? 9 : 3)
        let a = XCTAttachment(screenshot: app.screenshot())
        a.name = "investor-\(name)-\(appearance)-\(lang)"
        a.lifetime = .keepAlways
        add(a)
        app.terminate()
    }
}
