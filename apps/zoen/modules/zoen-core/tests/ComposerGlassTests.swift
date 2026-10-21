import XCTest
import UIKit
@testable import ZoenCore

// AUTHORED, NOT_RUN: needs a hosted iOS XCTest target with the actual Expo module.
// These verify native containment; they do not establish Fabric/RNGH pixel parity.
@MainActor
final class ComposerGlassTests: XCTestCase {
  func testChangingMaterialAndGeometryPreservesTheActualEditorAndItsSelection() {
    let host = ZoenComposerGlassSurface()
    host.frame = CGRect(x: 0, y: 0, width: 260, height: 74)
    let field = UITextView(frame: CGRect(x: 0, y: 0, width: 260, height: 74))
    field.text = "Same editor and caret"
    field.selectedRange = NSRange(location: 5, length: 6)
    let frame = field.frame
    host.mountChildComponentView(field, index: 0)
    let content = field.superview
    host.isDark = true; host.preference = "clear"; host.cornerRadius = 23
    host.apply(); host.layoutIfNeeded()
    XCTAssertTrue(host.children.first === field)
    XCTAssertTrue(field.superview === content)
    XCTAssertEqual(field.frame, frame, "Fabric alone owns the child's frame")
    XCTAssertEqual(field.text, "Same editor and caret")
    XCTAssertEqual(field.selectedRange, NSRange(location: 5, length: 6))
    XCTAssertEqual(host.glass.glassView.params?.tintColor.kind, .clear)
    XCTAssertEqual(host.glass.glassView.params?.isDark, true)
    host.preference = "panel"; host.isDark = false
    host.apply(); host.layoutIfNeeded()
    XCTAssertTrue(field.superview === content)
    XCTAssertEqual(host.glass.glassView.params?.tintColor.kind, .panel)
  }
  func testRetainedChildrenPreserveTheirOrderAndRejectAStaleUnmount() {
    let container = ZoenComposerGlassContainer()
    let first = UIView(frame: CGRect(x: 8, y: 0, width: 40, height: 40))
    let second = ZoenComposerGlassSurface()
    container.mountChildComponentView(first, index: 0)
    container.mountChildComponentView(second, index: 1)
    container.unmountChildComponentView(first, index: 1)
    XCTAssertTrue(container.children[0] === first)
    XCTAssertTrue(container.children[1] === second)
    XCTAssertTrue(first.superview === second.superview)
    container.isDark = true
    container.frame.size = CGSize(width: 402, height: 74)
    container.apply(); container.layoutIfNeeded()
    XCTAssertEqual(first.frame, CGRect(x: 8, y: 0, width: 40, height: 40))
    container.unmountChildComponentView(first, index: 0)
    XCTAssertNil(first.superview)
    XCTAssertTrue(container.children.first === second)
  }
  func testInvalidRadiusAndTintCannotReachTheOriginalRenderer() {
    let host = ZoenComposerGlassSurface()
    host.frame.size = CGSize(width: 40, height: 40)
    host.cornerRadius = .nan; host.preference = "invalid"
    host.apply(); host.layoutIfNeeded()
    XCTAssertEqual(host.glass.glassView.params?.tintColor.kind, .panel)
    XCTAssertEqual(host.glass.glassView.params?.shape, .roundedRect(cornerRadius: 20))
  }
}
