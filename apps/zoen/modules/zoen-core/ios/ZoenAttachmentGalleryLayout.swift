import Foundation
import CoreGraphics

/// Presentation decisions shared by the picker and its existing gallery.
/// Selection and delivery remain with the caller's signed draft/journal.
enum ZoenAttachmentGalleryLayout {
  static let pagePadding: CGFloat = 20
  static func prefersFullScreen(screen: String) -> Bool { usesPager(screen: screen) }
  static func usesPager(screen: String) -> Bool { screen == "preview" || screen == "selected" }

  // TGMediaPickerGalleryInterfaceView:400–420. The media moves with the
  // system keyboard's animation; caption and toolbar keep their own frames.
  static func mediaFrame(size: CGSize, keyboardHeight: CGFloat) -> CGRect {
    guard size.width.isFinite, size.height.isFinite, size.width > 0, size.height > 0,
      keyboardHeight.isFinite else { return .zero }
    return CGRect(x: 0, y: -min(size.height, max(0, keyboardHeight)) / 2,
      width: size.width, height: size.height)
  }

  static func aspectFit(dimensions: CGSize, viewport: CGRect) -> CGRect {
    guard dimensions.width.isFinite, dimensions.height.isFinite,
      dimensions.width > 0, dimensions.height > 0,
      viewport.origin.x.isFinite, viewport.origin.y.isFinite,
      viewport.width.isFinite, viewport.height.isFinite, viewport.width > 0, viewport.height > 0 else { return .zero }
    let scale = min(viewport.width / dimensions.width, viewport.height / dimensions.height)
    let size = CGSize(width: dimensions.width * scale, height: dimensions.height * scale)
    return CGRect(x: viewport.midX - size.width / 2, y: viewport.midY - size.height / 2,
      width: size.width, height: size.height)
  }

  static func reordered(_ ids: [String], from: Int, to: Int) -> [String]? {
    guard ids.count <= 10, Set(ids).count == ids.count,
      ids.indices.contains(from), ids.indices.contains(to) else { return nil }
    var result = ids; let id = result.remove(at: from); result.insert(id, at: to)
    return result
  }
}
