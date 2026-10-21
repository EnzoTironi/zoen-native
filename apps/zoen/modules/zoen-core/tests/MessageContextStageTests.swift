import XCTest
import UIKit
@testable import ZoenCore

// AUTHORED, NOT_RUN: requires a hosted iOS XCTest target and foreground UIWindowScene.
// These are native containment/lifetime assertions, not a Fabric/RNGH touch proof.
@MainActor
final class MessageContextStageTests: XCTestCase {
  private let theme: [String: Any] = ["background": "#FFFFFF", "highlight": "#EEEEEE", "text": "#000000", "secondary": "#777777", "separator": "#AAAAAA", "destructive": "#FF0000", "dim": "#00000080", "isDark": false]
  private func configuration(actions: [[String: String]] = [["id": "reply", "label": "Reply"]]) throws -> String {
    let data = try JSONSerialization.data(withJSONObject: ["actions": actions, "reactions": [["emoji": "❤️", "label": "Like", "selected": false]], "moreLabel": "More", "theme": theme, "reducedMotion": true, "incoming": true])
    return try XCTUnwrap(String(data: data, encoding: .utf8))
  }
  private struct Fixture {
    let window: UIWindow
    let parent: UIViewController
    let stage: ZoenMessageContextStage
    let source: ZoenMessageContextSource
    let child: UIView
    let binding: String
  }
  private func fixture(height: CGFloat = 80) throws -> Fixture {
    let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first { $0.activationState == .foregroundActive })
    let window = UIWindow(windowScene: scene)
    window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
    let parent = UIViewController()
    window.rootViewController = parent
    window.makeKeyAndVisible()
    let stage = ZoenMessageContextStage()
    stage.frame = parent.view.bounds
    parent.view.addSubview(stage)
    let source = ZoenMessageContextSource()
    let child = UIView(frame: CGRect(x: 0, y: 0, width: 240, height: height))
    source.frame = CGRect(x: 20, y: 160, width: 240, height: height)
    stage.mountChildComponentView(source, index: 0)
    source.mountChildComponentView(child, index: 0)
    let binding = UUID().uuidString
    source.bind(binding, active: true, handlesTap: true)
    stage.layoutIfNeeded(); source.layoutIfNeeded()
    return Fixture(window: window, parent: parent, stage: stage, source: source, child: child, binding: binding)
  }
  private func button(_ root: UIView, id: String) -> UIButton? {
    if let button = root as? UIButton, button.accessibilityIdentifier == id { return button }
    for child in root.subviews { if let match = button(child, id: id) { return match } }
    return nil
  }
  func testSelectedActionResolvesOnlyAfterSameHostRestoredAndControllerActuallyRemoved() async throws {
    let f = try fixture(), request = UUID().uuidString, config = try configuration()
    let task = Task { try await f.source.present(requestId: request, expectedBinding: f.binding, gestureId: "", configuration: config) }
    await Task.yield()
    let controller = try XCTUnwrap(f.stage.controller)
    XCTAssertTrue(f.source.contentHost.superview !== f.source)
    XCTAssertTrue(f.source.children.first === f.child)
    XCTAssertTrue(controller.parent === f.parent)
    try XCTUnwrap(button(controller.view, id: "message-context-reply")).sendActions(for: .touchUpInside)
    let selected = try await task.value
    XCTAssertEqual(selected, "reply")
    XCTAssertNil(controller.parent)
    XCTAssertNil(controller.view.superview)
    XCTAssertTrue(f.source.contentHost.superview === f.source)
    XCTAssertTrue(f.child.superview === f.source.contentHost)
    XCTAssertNil(f.stage.controller)
  }
  func testRetirementCancelsSelectionAndDrainsBeforeReturn() async throws {
    let f = try fixture(), config = try configuration()
    let task = Task { try await f.source.present(requestId: UUID().uuidString, expectedBinding: f.binding, gestureId: "", configuration: config) }
    await Task.yield()
    let controller = try XCTUnwrap(f.stage.controller)
    await f.stage.retireContexts()
    let selected = try await task.value
    XCTAssertNil(selected)
    XCTAssertNil(controller.parent)
    XCTAssertNil(controller.view.superview)
    XCTAssertFalse(f.stage.active)
    XCTAssertTrue(f.source.contentHost.superview === f.source)
  }
  func testRecycledBindingCannotOpenPreviousMessage() async throws {
    let f = try fixture()
    f.source.bind(UUID().uuidString, active: true, handlesTap: true)
    do {
      _ = try await f.source.present(requestId: UUID().uuidString, expectedBinding: f.binding, gestureId: "", configuration: configuration())
      XCTFail("stale binding must reject")
    } catch { XCTAssertNil(f.stage.controller) }
    XCTAssertTrue(f.child.superview === f.source.contentHost)
  }
  func testFabricChildUnmountSynchronouslyRestoresBeforeRemovingThatChild() async throws {
    let f = try fixture(), config = try configuration()
    let task = Task { try await f.source.present(requestId: UUID().uuidString, expectedBinding: f.binding, gestureId: "", configuration: config) }
    await Task.yield()
    f.source.unmountChildComponentView(f.child, index: 0)
    let selected = try await task.value
    XCTAssertNil(selected)
    XCTAssertNil(f.child.superview)
    XCTAssertTrue(f.source.children.isEmpty)
    XCTAssertNil(f.stage.controller)
  }
  func testTallSourceRetainsFullContentAndScrollableLiveHost() async throws {
    let f = try fixture(height: 1_200), config = try configuration()
    let task = Task { try await f.source.present(requestId: UUID().uuidString, expectedBinding: f.binding, gestureId: "", configuration: config) }
    await Task.yield()
    let preview = try XCTUnwrap(f.source.contentHost.superview as? UIScrollView)
    XCTAssertEqual(preview.contentSize.height, 1_200)
    XCTAssertLessThan(preview.bounds.height, preview.contentSize.height)
    XCTAssertTrue(f.source.children.first === f.child)
    await f.stage.retireContexts()
    _ = try await task.value
  }
  func testConfigurationCannotInventUnsupportedActionOrReaction() throws {
    XCTAssertThrowsError(try ZoenMessageContextConfiguration.decode(configuration(actions: [["id": "erase-account", "label": "Invalid"]])))
    var object = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(configuration().utf8)) as? [String: Any])
    object["reactions"] = [["emoji": "🔥", "label": "Invalid", "selected": false]]
    let data = try JSONSerialization.data(withJSONObject: object)
    XCTAssertThrowsError(try ZoenMessageContextConfiguration.decode(String(decoding: data, as: UTF8.self)))
  }
  func testInvalidThemeAndDuplicateActionsAreRejectedBeforeMovingAnyHost() throws {
    var object = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(configuration().utf8)) as? [String: Any])
    object["theme"] = ["background": "url(https://invalid)"]
    XCTAssertThrowsError(try ZoenMessageContextConfiguration.decode(String(decoding: JSONSerialization.data(withJSONObject: object), as: UTF8.self)))
    XCTAssertThrowsError(try ZoenMessageContextConfiguration.decode(configuration(actions: [["id": "reply", "label": "Reply"], ["id": "reply", "label": "Again"]])))
  }
  func testCustomReduceMotionNeverShrinksSourceOrRetiresItsGestureBinding() {
    let source = ZoenMessageContextSource()
    source.frame = CGRect(x: 0, y: 0, width: 240, height: 80)
    let binding = UUID().uuidString
    source.bind(binding, active: true, handlesTap: true, reducedMotion: true)
    let generation = source.generation
    source.press(1)
    XCTAssertEqual(source.contentHost.transform, .identity)
    XCTAssertNil(source.contentHost.layer.animationKeys())
    XCTAssertEqual(source.generation, generation)
    XCTAssertEqual(source.binding, binding)
    XCTAssertTrue(source.active)
  }
  func testEnablingCustomReduceMotionCancelsAnExistingPressImmediately() {
    let source = ZoenMessageContextSource()
    source.frame = CGRect(x: 0, y: 0, width: 240, height: 80)
    let binding = UUID().uuidString
    source.bind(binding, active: true, handlesTap: true)
    // Set an actual pre-existing visible press, independent of the test device's OS preference.
    source.contentHost.transform = CGAffineTransform(scaleX: 0.94, y: 0.94)
    let animation = CABasicAnimation(keyPath: "transform.scale")
    animation.fromValue = 0.94; animation.toValue = 1; animation.duration = 10
    source.contentHost.layer.add(animation, forKey: "existing.press")
    let generation = source.generation
    source.bind(binding, active: true, handlesTap: true, reducedMotion: true)
    XCTAssertEqual(source.contentHost.transform, .identity)
    XCTAssertNil(source.contentHost.layer.animationKeys())
    XCTAssertEqual(source.generation, generation)
    XCTAssertEqual(source.binding, binding)
  }
  func testCustomReduceMotionSuppressesAnimatedPressReset() {
    let source = ZoenMessageContextSource()
    source.frame = CGRect(x: 0, y: 0, width: 240, height: 80)
    source.bind(UUID().uuidString, active: true, handlesTap: true, reducedMotion: true)
    source.contentHost.transform = CGAffineTransform(scaleX: 0.94, y: 0.94)
    source.resetPress(animated: true)
    XCTAssertEqual(source.contentHost.transform, .identity)
    XCTAssertNil(source.contentHost.layer.animationKeys())
  }

  func testStationaryHeldReleaseOverRelocatedEditLeavesMenuOpen() async throws {
    let f = try fixture(), config = try configuration(actions: [["id": "reply", "label": "Reply"], ["id": "edit", "label": "Edit"]])
    f.source.frame.origin.y = f.window.bounds.height - f.window.safeAreaInsets.bottom - 100
    let task = Task { try await f.source.present(requestId: UUID().uuidString, expectedBinding: f.binding, gestureId: "", configuration: config) }
    await Task.yield()
    let controller = try XCTUnwrap(f.stage.controller)
    let edit = try XCTUnwrap(button(controller.view, id: "message-context-edit"))
    let origin = edit.convert(CGPoint(x: edit.bounds.midX, y: edit.bounds.midY), to: f.window)
    XCTAssertTrue(f.source.convert(f.source.bounds, to: f.window).contains(origin), "the moved Edit row overlaps the original held bubble")
    controller.trackHeldTouch(origin, origin: origin, ended: true, cancelled: false)
    XCTAssertTrue(f.stage.controller === controller, "stationary Up must leave the menu open")
    XCTAssertTrue(f.source.contentHost.superview !== f.source)
    await f.stage.retireContexts()
    let selected = try await task.value
    XCTAssertNil(selected)
  }

  func testHeldSelectionRequiresMoveBeyondStrictFourPointBoundary() async throws {
    let f = try fixture(), config = try configuration()
    let task = Task { try await f.source.present(requestId: UUID().uuidString, expectedBinding: f.binding, gestureId: "", configuration: config) }
    await Task.yield()
    let controller = try XCTUnwrap(f.stage.controller)
    let reply = try XCTUnwrap(button(controller.view, id: "message-context-reply"))
    let point = reply.convert(CGPoint(x: reply.bounds.midX, y: reply.bounds.midY), to: f.window)
    let origin = CGPoint(x: point.x, y: point.y - 4)
    controller.trackHeldTouch(point, origin: origin, ended: false, cancelled: false)
    controller.trackHeldTouch(point, origin: origin, ended: true, cancelled: false)
    XCTAssertTrue(f.stage.controller === controller, "4pt jitter is not a deliberate slide")
    await f.stage.retireContexts()
    let selected = try await task.value
    XCTAssertNil(selected)
  }

  func testHeldReleaseChoosesOnlyPreviouslyAdmittedHoverNotNewRowUnderUp() async throws {
    let f = try fixture(), config = try configuration(actions: [["id": "reply", "label": "Reply"], ["id": "edit", "label": "Edit"]])
    let task = Task { try await f.source.present(requestId: UUID().uuidString, expectedBinding: f.binding, gestureId: "", configuration: config) }
    await Task.yield()
    let controller = try XCTUnwrap(f.stage.controller)
    let reply = try XCTUnwrap(button(controller.view, id: "message-context-reply"))
    let edit = try XCTUnwrap(button(controller.view, id: "message-context-edit"))
    let point = reply.convert(CGPoint(x: reply.bounds.midX, y: reply.bounds.midY), to: f.window)
    let end = edit.convert(CGPoint(x: edit.bounds.midX, y: edit.bounds.midY), to: f.window)
    let origin = CGPoint(x: point.x, y: point.y - 5)
    controller.trackHeldTouch(point, origin: origin, ended: false, cancelled: false)
    controller.trackHeldTouch(end, origin: origin, ended: true, cancelled: false)
    let selected = try await task.value
    XCTAssertEqual(selected, "reply", "Up cannot retarget to Edit without a Move")
    XCTAssertNil(f.stage.controller)
    XCTAssertTrue(f.source.contentHost.superview === f.source)
  }

  func testCancelledHeldSlideClearsAdmissionAndCannotChooseOnLaterEnd() async throws {
    let f = try fixture(), config = try configuration()
    let task = Task { try await f.source.present(requestId: UUID().uuidString, expectedBinding: f.binding, gestureId: "", configuration: config) }
    await Task.yield()
    let controller = try XCTUnwrap(f.stage.controller)
    let reply = try XCTUnwrap(button(controller.view, id: "message-context-reply"))
    let point = reply.convert(CGPoint(x: reply.bounds.midX, y: reply.bounds.midY), to: f.window)
    let origin = CGPoint(x: point.x, y: point.y - 5)
    controller.trackHeldTouch(point, origin: origin, ended: false, cancelled: false)
    controller.trackHeldTouch(point, origin: origin, ended: true, cancelled: true)
    controller.trackHeldTouch(point, origin: origin, ended: true, cancelled: false)
    XCTAssertTrue(f.stage.controller === controller)
    await f.stage.retireContexts()
    let selected = try await task.value
    XCTAssertNil(selected)
  }

}
