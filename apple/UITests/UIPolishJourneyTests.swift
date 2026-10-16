import XCTest

final class UIPolishJourneyTests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    @MainActor
    private func launch(_ extra: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = [
            "-RodaDemo", "YES", "-RodaFreshStart", "YES", "-RodaResetDemo", "YES",
            "-RodaShowcase", "YES", "-RodaApprovalsExplained", "YES",
            "-AppleLanguages", "(pt-BR)", "-AppleLocale", "pt_BR", "-RodaAppearance", "light",
        ] + extra
        app.launch()
        return app
    }

    @MainActor
    private func keep(_ app: XCUIApplication, _ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = "ui-polish-\(name)"
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    @MainActor
    func testCaptureOnboardingAndChatsThroughTheLastRow() {
        let onboarding = launch(["-RodaOnboarding", "YES", "-RodaOnboardingStep", "0"])
        let hello = onboarding.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "Oi, eu sou")).firstMatch
        XCTAssertTrue(hello.waitForExistence(timeout: 20))
        // The secondary glass action remains available without reading the clipboard at launch.
        XCTAssertTrue(onboarding.staticTexts["Recebeu um convite?"].exists)
        let paste = onboarding.buttons["onboarding-paste-invite"]
        XCTAssertTrue(paste.exists && paste.isHittable)
        XCTAssertEqual(paste.label, "Colar")
        keep(onboarding, "onboarding")
        onboarding.terminate()

        let app = launch()
        XCTAssertTrue(app.buttons["newChat"].waitForExistence(timeout: 20))
        XCTAssertTrue(app.buttons["home-tile-Pedra Grande"].waitForExistence(timeout: 10))
        sleep(1)
        keep(app, "chats-list")
        let scroll = app.scrollViews["chats-list-scroll"]
        XCTAssertTrue(scroll.exists)
        let last = scroll.buttons["chat-row-last"]
        var previousBottom: CGFloat?
        var reachedBottom = false
        for _ in 0..<12 {
            scroll.swipeUp()
            guard last.exists, !last.frame.isEmpty else {
                previousBottom = nil
                continue
            }
            let bottom = last.frame.maxY
            if let previousBottom, abs(bottom - previousBottom) < 1 {
                reachedBottom = true
                break
            }
            previousBottom = bottom
        }
        keep(app, "chats-list-bottom")
        XCTAssertTrue(reachedBottom, "the last chat stops moving at the bottom of the list")
        XCTAssertTrue(last.exists && last.isHittable)
        let bar = app.otherElements["bottom-bar"]
        XCTAssertTrue(bar.exists)
        XCTAssertLessThanOrEqual(last.frame.maxY, bar.frame.minY,
                                 "the last chat can scroll fully above the floating bar")
    }

    @MainActor
    func testCaptureGroupThreadAndLongPressActions() {
        let app = launch(["-RodaOpen", "turma", "-RodaChatScrollTop", "YES"])
        let scroll = app.scrollViews["chat-message-scroll"]
        XCTAssertTrue(scroll.waitForExistence(timeout: 20))
        let chip = app.buttons["thread-chip"].firstMatch
        XCTAssertTrue(chip.waitForExistence(timeout: 10))
        sleep(2) // The demo's scroll-to-top task finishes before the journey starts scrolling.
        for _ in 0..<4 {
            if chip.exists && chip.isHittable { break }
            scroll.swipeUp()
        }
        XCTAssertTrue(chip.waitForExistence(timeout: 5) && chip.isHittable)
        XCTAssertFalse(app.buttons["message-actions"].exists, "messages have no per-bubble dots")
        keep(app, "group-chat")

        chip.tap()
        let close = app.buttons["thread-close"]
        XCTAssertTrue(close.waitForExistence(timeout: 5))
        let reply = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "Levo as frutas.")).firstMatch
        XCTAssertTrue(reply.waitForExistence(timeout: 5))
        sleep(1) // The thread focuses its composer after presentation.
        let keyboardTip = app.buttons["Continue"]
        if keyboardTip.waitForExistence(timeout: 3) {
            keyboardTip.tap()
            XCTAssertTrue(keyboardTip.waitForNonExistence(timeout: 5))
        }
        keep(app, "thread")
        close.tap()
        XCTAssertTrue(close.waitForNonExistence(timeout: 5))

        let message = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "Eu levo água e uns lanches")).firstMatch
        XCTAssertTrue(message.waitForExistence(timeout: 5) && message.isHittable)
        message.press(forDuration: 0.6)
        XCTAssertTrue(app.buttons["Responder"].waitForExistence(timeout: 5), "long-press opens message actions")
        keep(app, "message-actions")
        for label in ["Reagir com 👍", "Encaminhar", "Copiar", "Editar", "Apagar para todos"] {
            XCTAssertTrue(app.buttons[label].exists, "localized message action: \(label)")
        }
    }
}
