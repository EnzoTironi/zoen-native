import CoreGraphics
import Foundation
import AppKit
import PencilKit

@main struct PhotoEditorImageTests {
  enum Failure: Error { case invalid }
  static func image(_ values: [UInt8], width: Int, height: Int, transparent: Bool = false) throws -> CGImage {
    let bytes = values.flatMap { [$0, UInt8(0), UInt8(0), transparent && $0 == 0 ? UInt8(0) : UInt8(255)] }
    guard let provider = CGDataProvider(data: Data(bytes) as CFData),
          let image = CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
      bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
      bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
      provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent) else { throw Failure.invalid }
    return image
  }
  static func pixels(_ image: CGImage) throws -> [UInt8] {
    guard let data = image.dataProvider?.data, let bytes = CFDataGetBytePtr(data) else { throw Failure.invalid }
    return (0..<image.height).flatMap { y in
      (0..<image.width).map { x in bytes[y * image.bytesPerRow + x * 4] }
    }
  }
  static func refused(_ action: () throws -> Void) {
    do { try action(); preconditionFailure("Retired/invalid image operation was admitted") } catch { }
  }
  @MainActor static func actualPencilKitRecrop() throws {
    func overlay(_ size: CGSize, shade: CGFloat) throws -> CGImage {
      let points = [CGPoint(x: 40, y: 40), CGPoint(x: 160, y: 100)].enumerated().map { index, point in
        PKStrokePoint(location: point, timeOffset: TimeInterval(index), size: CGSize(width: 10, height: 10),
          opacity: 1, force: 1, azimuth: 0, altitude: .pi / 2)
      }
      let stroke = PKStroke(ink: PKInk(.pen, color: NSColor(srgbRed: shade, green: 0, blue: 0, alpha: 1)),
        path: PKStrokePath(controlPoints: points, creationDate: Date(timeIntervalSince1970: 0)))
      let bounds = CGRect(x: 0, y: 0, width: 370, height: size.height * 370 / size.width)
      let image = PKDrawing(strokes: [stroke]).image(from: bounds, scale: size.width / bounds.width)
      var rect = CGRect(origin: .zero, size: image.size)
      guard let cg = image.cgImage(forProposedRect: &rect, context: nil, hints: nil) else { throw Failure.invalid }
      return cg
    }
    // Same aspect as QA's 370x246.9133 fitted image, with fractional
    // fitted bounds after the cropped quarter turn as well.
    guard let context = CGContext(data: nil, width: 3000, height: 2002, bitsPerComponent: 8,
      bytesPerRow: 3000 * 4, space: CGColorSpaceCreateDeviceRGB(),
      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue),
          let original = context.makeImage() else { throw Failure.invalid }
    let recipe = ZoenPhotoEditorImage(original)
    defer { recipe.release() }
    let first = try recipe.commitInk(overlay(CGSize(width: 3000, height: 2002), shade: 1))
    let firstPixels = try pixels(first)
    precondition(firstPixels.contains { $0 > 200 })
    try recipe.crop(to: CGRect(x: 0.08, y: 0.06, width: 0.92, height: 0.93))
    let turned = try recipe.rotate()
    let before = try pixels(turned)
    let ink = try overlay(CGSize(width: turned.width, height: turned.height), shade: 0.5)
    // PencilKit's chosen RGB profile varies by OS (SDR on hosted macOS 15,
    // extended-range on macOS 27). Both must preserve actual ink through recrop.
    guard let space = ink.colorSpace else { throw Failure.invalid }
    precondition(space.model == .rgb)
    let second = try recipe.commitInk(ink)
    let secondPixels = try pixels(second)
    precondition(secondPixels != before)
    precondition(recipe.canReset)
    let reset = try recipe.reset()
    precondition(reset.width == 3000 && reset.height == 2002 && !recipe.canReset)
    let resetPixels = try pixels(reset)
    precondition(resetPixels.contains { $0 > 200 })
    // Re-entering Crop and rendering Save bytes must remain admitted too.
    try recipe.crop(to: CGRect(x: 0, y: 0, width: 1, height: 1))
    let reopened = try pixels(recipe.rendered())
    precondition(reopened == resetPixels)
  }
  static func explicitExtendedRangeRecrop() throws {
    // Exercise the production fallback regardless of PencilKit's OS profile.
    guard let space = CGColorSpace(name: CGColorSpace.extendedSRGB),
          let context = CGContext(data: nil, width: 2, height: 3, bitsPerComponent: 32,
            bytesPerRow: 2 * 16, space: space,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
              | CGBitmapInfo.floatComponents.rawValue | CGBitmapInfo.byteOrder32Little.rawValue),
          let red = CGColor(colorSpace: space, components: [1, 0, 0, 1]) else { throw Failure.invalid }
    context.setFillColor(red)
    context.fill(CGRect(x: 0, y: 0, width: 2, height: 3))
    guard let ink = context.makeImage(), let profile = ink.colorSpace else { throw Failure.invalid }
    precondition(CGColorSpaceUsesExtendedRange(profile) && ink.bitsPerComponent == 32)
    let original = try image(Array(repeating: 0, count: 15), width: 5, height: 3)
    let recipe = ZoenPhotoEditorImage(original)
    defer { recipe.release() }
    try recipe.crop(to: CGRect(x: 0.2, y: 1.0 / 3, width: 0.6, height: 2.0 / 3))
    let turned = try recipe.rotate()
    precondition(turned.width == 2 && turned.height == 3)
    let committed = try recipe.commitInk(ink)
    let visible = try pixels(committed)
    precondition(visible == Array(repeating: 255, count: 6))
    let restored = try recipe.reset()
    let full = try pixels(restored)
    precondition(full == [0,0,0,0,0, 0,255,255,255,0, 0,255,255,255,0])
    precondition(!recipe.canReset)
    try recipe.crop(to: CGRect(x: 0, y: 0, width: 1, height: 1))
    let reopened = try pixels(recipe.rendered())
    precondition(reopened == full)
  }
  static func stage(_ label: String, since start: Date) {
    let elapsed = String(format: "%.3f", Date().timeIntervalSince(start))
    // Write directly so a terminated hosted process still identifies its last stage.
    FileHandle.standardError.write(Data("Photo editor pixels: \(label) at \(elapsed)s\n".utf8))
  }
  @MainActor static func main() throws {
    let start = Date()
    stage("PencilKit start", since: start)
    try actualPencilKitRecrop()
    stage("PencilKit complete; extended RGB start", since: start)
    try explicitExtendedRangeRecrop()
    stage("extended RGB complete; geometry start", since: start)

    // Odd, asymmetric dimensions expose half-pixel centering and turn direction.
    let odd = try image(Array(1...15), width: 5, height: 3)
    let rotations: [[UInt8]] = [Array(1...15),
      [11,6,1,12,7,2,13,8,3,14,9,4,15,10,5],
      Array((1...15).reversed()), [5,10,15,4,9,14,3,8,13,2,7,12,1,6,11]]
    let leftColumns: [[UInt8]] = [[1,6,11], [11,12,13,14,15], [15,10,5], [5,4,3,2,1]]
    for turns in 0..<4 {
      let current = ZoenPhotoEditorImage(odd)
      for _ in 0..<turns { try current.rotate() }
      let visible = try current.rendered()
      let actual = try pixels(visible)
      precondition(actual == rotations[turns])
      let cropped = try current.crop(to: CGRect(x: 0, y: 0, width: 1 / CGFloat(visible.width), height: 1))
      let column = try pixels(cropped)
      precondition(column == leftColumns[turns])
      let restored = try current.reset()
      let full = try pixels(restored)
      precondition(full == Array(1...15))
      current.release()
    }
    let fourTurns = ZoenPhotoEditorImage(odd)
    for _ in 0..<4 { try fourTurns.rotate() }
    let identity = try pixels(fourTurns.rendered())
    precondition(identity == Array(1...15))
    fourTurns.release()
    let original = try image(Array(1...12), width: 4, height: 3)
    let recipe = ZoenPhotoEditorImage(original)
    func expect(_ values: [UInt8], _ message: String = "Unexpected pixels") throws {
      let actual = try pixels(recipe.rendered())
      precondition(actual == values, message + ": \(actual)")
    }
    try expect(Array(1...12))
    try recipe.rotate()
    try expect([9,5,1,10,6,2,11,7,3,12,8,4], "Quarter turn changed pixel orientation")
    try recipe.crop(to: CGRect(x: 0, y: 0, width: 1.0/3, height: 1))
    try expect([9,10,11,12], "Crop did not invert displayed rotation")
    try recipe.crop(to: CGRect(x: 0, y: 0, width: 1, height: 0.5))
    try expect([9,10])
    try recipe.reset()
    precondition(!recipe.canReset)
    try expect(Array(1...12), "Reset must restore full framing and upright orientation")

    // Ink painted on a cropped/rotated preview must survive Reset in its source location.
    try recipe.crop(to: CGRect(x: 0.25, y: 0, width: 0.5, height: 1))
    try recipe.rotate()
    let overlay = try image([200,0,0,0,0,0], width: 3, height: 2, transparent: true)
    try recipe.commitInk(overlay)
    try expect([200,6,2,11,7,3], "Preview lost ink or neighboring pixels")
    try recipe.reset()
    try expect([1,2,3,4,5,6,7,8,9,200,11,12], "Reset erased or misplaced recent ink")

    // Repeat in the opposite orientation, preserving the earlier annotation too.
    try recipe.crop(to: CGRect(x: 0.25, y: 0, width: 0.5, height: 1))
    for _ in 0..<3 { try recipe.rotate() }
    try recipe.commitInk(image([220,0,0,0,0,0], width: 3, height: 2, transparent: true))
    try recipe.reset()
    try expect([1,2,220,4,5,6,7,8,9,200,11,12])
    let before = try pixels(recipe.rendered())
    refused { try recipe.commitInk(image([250], width: 1, height: 1)) }
    try expect(before)
    for rect in [CGRect(x: -0.1, y: 0, width: 1, height: 1),
                 CGRect(x: 0, y: 0, width: 0, height: 1),
                 CGRect(x: 0, y: 0, width: CGFloat.infinity, height: 1),
                 CGRect(x: 0.5, y: 0, width: 1, height: 1)] {
      refused { try recipe.crop(to: rect) }
      try expect(before)
    }
    recipe.release()
    precondition(!recipe.canReset)
    refused { _ = try recipe.rendered() }
    refused { try recipe.reset() }
    refused { try recipe.rotate() }
    refused { try recipe.crop(to: CGRect(x: 0, y: 0, width: 1, height: 1)) }
    refused { try recipe.commitInk(overlay) }
    stage("geometry complete", since: start)
    print("Photo editor pixels PASS: crop/rotation, Reset, ink, bounds, retirement")
  }
}
