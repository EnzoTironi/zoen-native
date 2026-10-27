import CoreGraphics
import Foundation

/// Session-local crop geometry. Only rendered bytes enter the existing media journal.
/// Ink is committed in source coordinates so resetting framing never erases it.
/// Reset follows Telegram-iOS f1dd7a2 TGPhotoCropView.m:1018–1068:
/// full source rectangle and upright orientation, independent of annotation.
final class ZoenPhotoEditorImage {
  enum Failure: Error { case invalid }
  private var source: CGImage?
  private(set) var cropRect: CGRect
  private(set) var quarterTurns = 0

  init(_ image: CGImage) {
    source = image
    cropRect = CGRect(x: 0, y: 0, width: image.width, height: image.height)
  }
  var canReset: Bool {
    guard let source else { return false }
    return quarterTurns != 0 || cropRect != CGRect(x: 0, y: 0, width: source.width, height: source.height)
  }
  func rendered() throws -> CGImage {
    guard let source else { throw Failure.invalid }
    return try Self.render(source, crop: cropRect, turns: quarterTurns)
  }
  @discardableResult func crop(to normalized: CGRect) throws -> CGImage {
    guard let source, normalized.origin.x.isFinite, normalized.origin.y.isFinite,
          normalized.width.isFinite, normalized.height.isFinite,
          normalized.minX >= 0, normalized.minY >= 0,
          normalized.maxX <= 1, normalized.maxY <= 1,
          normalized.width > 0, normalized.height > 0 else { throw Failure.invalid }
    let w = cropRect.width, h = cropRect.height
    let width = quarterTurns % 2 == 0 ? w : h
    let height = quarterTurns % 2 == 0 ? h : w
    let visible = CGRect(x: normalized.minX * width, y: normalized.minY * height,
      width: normalized.width * width, height: normalized.height * height).integral
      .intersection(CGRect(x: 0, y: 0, width: width, height: height))
    let local: CGRect
    switch quarterTurns {
    case 1: local = CGRect(x: visible.minY, y: h - visible.maxX, width: visible.height, height: visible.width)
    case 2: local = CGRect(x: w - visible.maxX, y: h - visible.maxY, width: visible.width, height: visible.height)
    case 3: local = CGRect(x: w - visible.maxY, y: visible.minX, width: visible.height, height: visible.width)
    default: local = visible
    }
    guard !local.isEmpty else { throw Failure.invalid }
    let next = local.offsetBy(dx: cropRect.minX, dy: cropRect.minY)
    let image = try Self.render(source, crop: next, turns: quarterTurns)
    cropRect = next
    return image
  }
  @discardableResult func rotate() throws -> CGImage {
    guard let source else { throw Failure.invalid }
    let next = (quarterTurns + 1) % 4
    let image = try Self.render(source, crop: cropRect, turns: next)
    quarterTurns = next
    return image
  }
  @discardableResult func reset() throws -> CGImage {
    guard let source else { throw Failure.invalid }
    cropRect = CGRect(x: 0, y: 0, width: source.width, height: source.height)
    quarterTurns = 0
    return source
  }
  @discardableResult func commitInk(_ overlay: CGImage) throws -> CGImage {
    guard let source else { throw Failure.invalid }
    let displayWidth = quarterTurns % 2 == 0 ? cropRect.width : cropRect.height
    let displayHeight = quarterTurns % 2 == 0 ? cropRect.height : cropRect.width
    // PencilKit rasterizes the fitted bounds at source pixel scale. Allow its
    // one-pixel edge rounding, never an unrelated/downsampled display buffer.
    guard abs(CGFloat(overlay.width) - displayWidth) <= 1,
          abs(CGFloat(overlay.height) - displayHeight) <= 1 else { throw Failure.invalid }
    let unrotated = try Self.rotated(overlay, turns: (4 - quarterTurns) % 4)
    guard let context = Self.context(width: source.width, height: source.height, colorSpace: source.colorSpace) else { throw Failure.invalid }
    context.draw(source, in: CGRect(x: 0, y: 0, width: source.width, height: source.height))
    context.draw(unrotated, in: CGRect(x: cropRect.minX, y: CGFloat(source.height) - cropRect.maxY,
      width: cropRect.width, height: cropRect.height))
    guard let composed = context.makeImage() else { throw Failure.invalid }
    let image = try Self.render(composed, crop: cropRect, turns: quarterTurns)
    self.source = composed
    return image
  }
  func release() { source = nil }

  private static func render(_ source: CGImage, crop: CGRect, turns: Int) throws -> CGImage {
    guard let image = source.cropping(to: crop) else { throw Failure.invalid }
    return try rotated(image, turns: turns)
  }
  private static func context(width: Int, height: Int, colorSpace: CGColorSpace?) -> CGContext? {
    let space: CGColorSpace
    if let colorSpace, colorSpace.model == .rgb, !CGColorSpaceUsesExtendedRange(colorSpace) {
      space = colorSpace
    } else {
      // PencilKit can return extended-range RGB even for SDR ink. That
      // profile is incompatible with this bounded 8-bit output context;
      // CoreGraphics converts it to sRGB while drawing, before rotation.
      guard let standard = CGColorSpace(name: CGColorSpace.sRGB) else { return nil }
      space = standard
    }
    return CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
      space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
  }
  private static func rotated(_ image: CGImage, turns: Int) throws -> CGImage {
    if turns == 0 { return image }
    let width = turns % 2 == 0 ? image.width : image.height
    let height = turns % 2 == 0 ? image.height : image.width
    guard let context = context(width: width, height: height, colorSpace: image.colorSpace) else { throw Failure.invalid }
    context.interpolationQuality = .none
    context.translateBy(x: CGFloat(width) / 2, y: CGFloat(height) / 2)
    context.rotate(by: -CGFloat(turns) * .pi / 2)
    context.draw(image, in: CGRect(x: -CGFloat(image.width) / 2, y: -CGFloat(image.height) / 2,
      width: CGFloat(image.width), height: CGFloat(image.height)))
    guard let result = context.makeImage() else { throw Failure.invalid }
    return result
  }
}
