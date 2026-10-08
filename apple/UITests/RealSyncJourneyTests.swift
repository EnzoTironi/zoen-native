import XCTest

/// Milestone 1, on a simulator: a fresh account on this iPhone finds a person by @handle,
/// talks to them through the real relay, and still has the chat after a relaunch.
/// The other person is the `zoen` CLI; `scripts/journey-sim.sh` starts the relay, plays
/// them, and passes the handles in (ZOEN_ME, ZOEN_PEER).
final class RealSyncJourneyTests: XCTestCase {
    @MainActor
    func testChatWithAPersonThroughTheRelayAndRelaunch() throws {
        continueAfterFailure = false
        let env = ProcessInfo.processInfo.environment
        guard let me = env["ZOEN_ME"], let peer = env["ZOEN_PEER"] else {
            throw XCTSkip("needs a relay and a peer: run scripts/journey-sim.sh")
        }
        let relay = env["ZOEN_RELAY"] ?? "http://127.0.0.1:8787"
        let mine = env["ZOEN_MESSAGE"] ?? "oi Bruno, é a Ana no simulador"
        let reply = env["ZOEN_REPLY"] ?? "oi Ana, aqui é o Bruno no terminal"

        let app = XCUIApplication()
        app.launchArguments = ["-RodaFreshStart", "YES", "-RodaAccount", "Ana:\(me)", "-RodaRelay", relay, "-AppleLanguages", "(en)"]
        app.launch()

        // New chat → find @peer → open the DM.
        let compose = app.buttons["newChat"]
        XCTAssertTrue(compose.waitForExistence(timeout: 15), "the compose button shows once there's an account")
        compose.tap()
        let search = app.textFields["Search by @handle"]
        XCTAssertTrue(search.waitForExistence(timeout: 5))
        search.tap()
        search.typeText(peer)
        let person = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "@\(peer)")).firstMatch
        XCTAssertTrue(person.waitForExistence(timeout: 20), "the relay's directory finds @\(peer)")
        person.tap()

        // Say hi; the peer answers from the terminal.
        let composer = app.descendants(matching: .any)["composer"]
        XCTAssertTrue(composer.waitForExistence(timeout: 10))
        composer.tap()
        composer.typeText(mine)
        // The keyboard's return key is also a "send" button (label "send", identifier "Send");
        // the composer's is identifier "send", label "Send". Compare case-sensitively.
        app.buttons.matching(NSPredicate(format: "identifier ==[c] 'send' AND label == 'Send'")).firstMatch.tap()
        XCTAssertTrue(app.staticTexts[mine].waitForExistence(timeout: 5))
        let replyWait = Double(env["ZOEN_REPLY_TIMEOUT"] ?? "") ?? 45
        XCTAssertTrue(app.staticTexts[reply].waitForExistence(timeout: replyWait), "the reply arrives through the relay")
        keep(app, "ana: reply arrived")

        // Kill and relaunch: the account, the chat and both messages are still here.
        app.terminate()
        app.launchArguments = ["-RodaRelay", relay, "-AppleLanguages", "(en)"]
        app.launch()
        let chat = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", reply)).firstMatch
        let found = chat.waitForExistence(timeout: 15)
        if !found { keep(app, "after relaunch") }
        XCTAssertTrue(found, "the chat with the peer is in the list after relaunch")
        chat.tap()
        XCTAssertTrue(app.staticTexts[mine].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts[reply].exists)
        keep(app, "ana: after relaunch")
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
