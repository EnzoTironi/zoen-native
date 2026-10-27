import CoreGraphics
import Foundation

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
  static func main() throws {
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
    print("Photo editor pixels PASS: crop/rotation, Reset, ink, bounds, retirement")
  }
}
