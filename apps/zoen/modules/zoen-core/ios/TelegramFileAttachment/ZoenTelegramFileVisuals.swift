import UIKit

// Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838,
// ListMessageFileItemNode.swift:26–115,637–658,1048–1058,1107–1120,
// 1240,1282–1319. GPL-2.0-or-later; GPLv3 selected.
// Original paths/colors and layout math; public UIKit replaces Display/TextNode.
@MainActor enum ZoenTelegramFileVisuals {
  static func documentIcon(_ fileExtension: String) -> UIImage {
    let colors: (UInt32, UInt32)
    switch fileExtension {
    case "ppt", "pptx", "pdf", "key": colors = (0xff875f, 0xff5069)
    case "xls", "xlsx", "csv": colors = (0x99de6f, 0x5fb84f)
    case "zip", "rar", "gzip", "ai": colors = (0xffa24b, 0xed705c)
    default: colors = (0x72d5fd, 0x2a9ef1)
    }
    return UIGraphicsImageRenderer(size: CGSize(width: 40, height: 40)).image { value in
      let context = value.cgContext
      let outline = "M6,0 L26.7573593,0 C27.5530088,-8.52837125e-16 28.3160705,0.316070521 28.8786797,0.878679656 L39.1213203,11.1213203 C39.6839295,11.6839295 40,12.4469912 40,13.2426407 L40,34 C40,37.3137085 37.3137085,40 34,40 L6,40 C2.6862915,40 4.05812251e-16,37.3137085 0,34 L0,6 C-4.05812251e-16,2.6862915 2.6862915,6.08718376e-16 6,0"
      context.saveGState(); context.addPath(path(outline)); context.clip()
      let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: [color(colors.0).cgColor, color(colors.1).cgColor] as CFArray, locations: [0, 1])!
      context.drawLinearGradient(gradient, start: .zero, end: CGPoint(x: 0, y: 40), options: [])
      context.restoreGState(); context.saveGState()
      context.addPath(UIBezierPath(rect: CGRect(x: 0, y: 0, width: 40, height: 40)).cgPath)
      context.addPath(UIBezierPath(roundedRect: CGRect(x: 0, y: 0, width: 40, height: 40), cornerRadius: 13).cgPath)
      context.setBlendMode(.destinationOut); context.drawPath(using: .eoFill); context.restoreGState()
      context.saveGState(); context.addPath(path(outline)); context.clip(); context.setBlendMode(.overlay)
      context.setFillColor(UIColor.white.withAlphaComponent(0.5).cgColor); context.translateBy(x: 26, y: 0)
      context.addPath(path("M-1,0 L14,0 L14,15 L14,14 C14,12.8954305 13.1045695,12 12,12 L4,12 C2.8954305,12 2,11.1045695 2,10 L2,2 C2,0.8954305 1.1045695,-2.02906125e-16 0,0 L-1,0 L-1,0 Z")); context.fillPath(); context.restoreGState()
    }
  }
  private static func color(_ value: UInt32) -> UIColor {
    UIColor(red: CGFloat((value >> 16) & 255) / 255, green: CGFloat((value >> 8) & 255) / 255, blue: CGFloat(value & 255) / 255, alpha: 1)
  }
  /// Only the original document's M/L/C/Z grammar is accepted, no general SVG engine.
  private static func path(_ value: String) -> CGPath {
    let expression = try! NSRegularExpression(pattern: "[MLCZ]|[-+]?(?:[0-9]*\\.)?[0-9]+(?:[eE][-+]?[0-9]+)?")
    let tokens = expression.matches(in: value, range: NSRange(value.startIndex..., in: value)).map { (value as NSString).substring(with: $0.range) }
    let result = CGMutablePath(); var index = 0
    func point() -> CGPoint { let x = Double(tokens[index])!, y = Double(tokens[index + 1])!; index += 2; return CGPoint(x: x, y: y) }
    while index < tokens.count {
      let command = tokens[index]; index += 1
      switch command {
      case "M": result.move(to: point())
      case "L": result.addLine(to: point())
      case "C": let a = point(), b = point(), c = point(); result.addCurve(to: c, control1: a, control2: b)
      case "Z": result.closeSubpath()
      default: preconditionFailure("Unexpected fixed upstream document path")
      }
    }
    return result
  }
  static func searchIcon() -> UIImage? {
    let owner = Bundle(for: BundleMarker.self)
    guard let bundleURL = owner.url(forResource: "ZoenTelegramFileAttachment", withExtension: "bundle"), let bundle = Bundle(url: bundleURL),
          let url = bundle.url(forResource: "search", withExtension: "pdf"), let document = CGPDFDocument(url as CFURL), let page = document.page(at: 1) else { return nil }
    let bounds = page.getBoxRect(.mediaBox)
    return UIGraphicsImageRenderer(size: bounds.size).image { value in
      value.cgContext.translateBy(x: 0, y: bounds.height); value.cgContext.scaleBy(x: 1, y: -1)
      value.cgContext.drawPDFPage(page)
    }.withRenderingMode(.alwaysTemplate)
  }
  // AttachmentFileSearchItem:1154–1174, original 40pt cross path.
  static func searchCloseIcon() -> UIImage {
    UIGraphicsImageRenderer(size: CGSize(width: 40, height: 40)).image { value in
      let context = value.cgContext
      context.setLineWidth(2); context.setLineCap(.round); context.setStrokeColor(UIColor.white.cgColor)
      context.move(to: CGPoint(x: 12, y: 12)); context.addLine(to: CGPoint(x: 28, y: 28))
      context.move(to: CGPoint(x: 28, y: 12)); context.addLine(to: CGPoint(x: 12, y: 28)); context.strokePath()
    }.withRenderingMode(.alwaysTemplate)
  }
  private final class BundleMarker: NSObject {}
}

struct ZoenTelegramFileRowLayout {
  let height: CGFloat
  let title: CGRect
  let subtitle: CGRect
  let icon: CGRect
  let selection: CGRect
  let separator: CGRect
  static func measure(width: CGFloat, left: CGFloat, right: CGFloat, titleHeight: CGFloat, subtitleHeight: CGFloat, scale: CGFloat, selection: Bool) -> Self {
    let pixel = 1 / max(1, scale), vertical = 2 + pixel, offset: CGFloat = selection ? 45 : 0
    let titleY = 7 + vertical
    let titleFrame = CGRect(x: left + offset + 64, y: titleY, width: max(0, width - left - right - offset - 65 - 8), height: titleHeight)
    let subtitleFrame = CGRect(x: titleFrame.minX, y: titleFrame.maxY - 3, width: max(0, width - left - right - offset - 65 - 30), height: subtitleHeight)
    let height = vertical * 2 + 16 + titleHeight - 5 + subtitleHeight
    return Self(height: height, title: titleFrame, subtitle: subtitleFrame,
      icon: CGRect(x: left + offset + 12, y: 8 + vertical, width: 40, height: 40),
      selection: CGRect(x: left + 13, y: floor((height - 26) / 2 * max(1, scale)) / max(1, scale), width: 26, height: 26),
      separator: CGRect(x: left + offset + 65, y: height - pixel, width: max(0, width - left - right - offset - 65 - 16), height: pixel))
  }
}
