import UIKit

/// Original pinned vectors/bitmap. The host can supply the same glyphs from its
/// existing presentation bundle instead of loading this optional resource bundle.
@MainActor struct ZoenTelegramVideoGlyphs {
  let play: UIImage
  let mute: UIImage
  let gif: UIImage

  static func bundled() throws -> Self {
    let containing = Bundle(for: BundleMarker.self)
    guard let url = containing.url(forResource: "ZoenTelegramVideoEditing", withExtension: "bundle"),
          let bundle = Bundle(url: url),
          let muteURL = bundle.url(forResource: "Mute@2x", withExtension: "png"),
          let bitmap = UIImage(contentsOfFile: muteURL.path), let cgImage = bitmap.cgImage else {
      throw ZoenTelegramVideoFailure.missingResources
    }
    func vector(_ name: String) throws -> UIImage {
      guard let url = bundle.url(forResource: name, withExtension: "pdf"),
            let document = CGPDFDocument(url as CFURL), let page = document.page(at: 1) else {
        throw ZoenTelegramVideoFailure.missingResources
      }
      let bounds = page.getBoxRect(.mediaBox)
      guard bounds.width > 0, bounds.height > 0, bounds.width <= 128, bounds.height <= 128 else {
        throw ZoenTelegramVideoFailure.missingResources
      }
      return UIGraphicsImageRenderer(size: bounds.size).image { value in
        value.cgContext.translateBy(x: 0, y: bounds.height)
        value.cgContext.scaleBy(x: 1, y: -1)
        value.cgContext.translateBy(x: -bounds.minX, y: -bounds.minY)
        value.cgContext.drawPDFPage(page)
      }.withRenderingMode(.alwaysTemplate)
    }
    return try Self(play: vector("Play"),
      mute: UIImage(cgImage: cgImage, scale: 2, orientation: .up).withRenderingMode(.alwaysTemplate),
      gif: vector("Gif"))
  }
  private final class BundleMarker: NSObject {}
}

