import ExpoModulesCore
import UIKit

// Original Zoen rendering of observed Telegram iOS f1dd7a2 geometry.
// Renderer/lifecycle are Zoen code; derived glyph data is explicitly isolated and licensed in ZoenTelegramCheckGeometry.swift.
// No Telegram engine, image/font/logo assets, or network state is embedded. Not a clean-room claim.
@MainActor private enum ZoenMessageInk {
  static func color(_ value: String) -> UIColor? {
    guard value.first == "#" else { return nil }
    let text = String(value.dropFirst())
    guard text.count == 6 || text.count == 8, let number = UInt64(text, radix: 16) else { return nil }
    let rgb = text.count == 8 ? number >> 8 : number
    return UIColor(red: CGFloat((rgb >> 16) & 255) / 255,
                   green: CGFloat((rgb >> 8) & 255) / 255,
                   blue: CGFloat(rgb & 255) / 255,
                   alpha: text.count == 8 ? CGFloat(number & 255) / 255 : 1)
  }

  static func corners(_ values: [CGFloat]) -> UIBezierPath {
    let path = UIBezierPath()
    let centers = [CGPoint(x: 33 - values[1], y: values[1]),
                   CGPoint(x: 33 - values[3], y: 33 - values[3]),
                   CGPoint(x: values[2], y: 33 - values[2]), CGPoint(x: values[0], y: values[0])]
    let radii = [values[1], values[3], values[2], values[0]]
    path.move(to: CGPoint(x: values[0], y: 0))
    for index in 0..<4 {
      let start = CGFloat(index - 1) * .pi / 2
      path.addArc(withCenter: centers[index], radius: radii[index], startAngle: start,
                  endAngle: start + .pi / 2, clockwise: true)
    }
    path.close()
    return path
  }

  static func bubble(radii: [CGFloat], tail: Bool, incoming: Bool, color: UIColor, scale: CGFloat) -> UIImage {
    let format = UIGraphicsImageRendererFormat()
    format.opaque = false
    format.scale = scale
    let bitmap = UIGraphicsImageRenderer(size: CGSize(width: 43, height: 37), format: format).image { surface in
      let context = surface.cgContext
      if incoming { context.translateBy(x: 43, y: 0); context.scaleBy(x: -1, y: 1) }
      context.translateBy(x: 2, y: 2)
      color.setFill()
      corners(radii).fill()
      if tail {
        // The lower lobe is two quadratic halves; the upper ellipse subtracts it.
        let lower = CGRect(x: 24, y: 16, width: 27, height: 17)
        let lobe = UIBezierPath()
        lobe.move(to: CGPoint(x: lower.minX, y: lower.midY))
        for (end, control) in [(CGPoint(x: lower.midX, y: lower.maxY), CGPoint(x: lower.minX, y: lower.maxY)),
                               (CGPoint(x: lower.maxX, y: lower.midY), CGPoint(x: lower.maxX, y: lower.maxY))] {
          lobe.addQuadCurve(to: end, controlPoint: control)
        }
        lobe.close(); lobe.fill()
        context.fill(CGRect(x: 16.5, y: 16, width: 16.5, height: 9))
        context.setBlendMode(.clear)
        context.fillEllipse(in: CGRect(x: 33, y: 14, width: 23, height: 21))
      }
    }
    let left: CGFloat = incoming ? 24 : 18
    return bitmap.resizableImage(withCapInsets: UIEdgeInsets(top: 19, left: left, bottom: 17, right: 42 - left), resizingMode: .stretch)
  }

  static func check(partial: Bool, color: UIColor, scale: CGFloat) -> UIImage {
    let format = UIGraphicsImageRendererFormat(); format.opaque = false; format.scale = scale
    return UIGraphicsImageRenderer(size: CGSize(width: 11, height: 9), format: format).image { surface in
      let path = UIBezierPath()
      path.lineWidth = 0.99; path.lineCapStyle = .round; path.lineJoinStyle = .round
      surface.cgContext.translateBy(x: 1, y: 1)
      for segment in ZoenTelegramCheckGeometry.segments(partial: partial) {
        switch segment {
        case let .move(point): path.move(to: point)
        case let .line(point): path.addLine(to: point)
        case let .cubic(end, first, second): path.addCurve(to: end, controlPoint1: first, controlPoint2: second)
        }
      }
      color.setStroke(); path.stroke()
    }
  }

  static func clock(hand: Bool, color: UIColor, scale: CGFloat) -> UIImage {
    let format = UIGraphicsImageRendererFormat(); format.opaque = false; format.scale = scale
    return UIGraphicsImageRenderer(size: CGSize(width: 11, height: 11), format: format).image { surface in
      let context = surface.cgContext
      context.setFillColor(color.cgColor); context.setStrokeColor(color.cgColor); context.setLineWidth(1)
      if hand { context.fill(CGRect(x: 5, y: 5, width: 4.5, height: 1)) }
      else { context.strokeEllipse(in: CGRect(x: 0.5, y: 0.5, width: 10, height: 10)); context.fill(CGRect(x: 5, y: 3, width: 1, height: 2.5)) }
    }
  }
}

@MainActor final class ZoenMessageBubbleView: ExpoView {
  var ownerKey = ""
  var incoming = false
  var tail = false
  var topLeft = 16.0
  var topRight = 16.0
  var bottomLeft = 16.0
  var bottomRight = 16.0
  var fill = ""
  var fillColors: [String] = []
  var animateChanges = false
  private let image = UIImageView()
  private let colorView = UIView()
  private let gradient = CAGradientLayer()
  private var appliedKey = ""
  private var appliedOwner = ""

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    isUserInteractionEnabled = false; isAccessibilityElement = false; clipsToBounds = false
    image.isUserInteractionEnabled = false; image.isAccessibilityElement = false
    colorView.isUserInteractionEnabled = false; colorView.isAccessibilityElement = false
    gradient.startPoint = CGPoint(x: 0.5, y: 0); gradient.endPoint = CGPoint(x: 0.5, y: 1)
    colorView.layer.addSublayer(gradient); colorView.mask = image
    addSubview(colorView)
    NotificationCenter.default.addObserver(self, selector: #selector(reduceMotionChanged), name: UIAccessibility.reduceMotionStatusDidChangeNotification, object: nil)
  }
  @objc private func reduceMotionChanged() { if UIAccessibility.isReduceMotionEnabled { image.layer.removeAllAnimations() } }
  func apply() {
    let raw = [topLeft, topRight, bottomLeft, bottomRight]
    guard !ownerKey.isEmpty, raw.allSatisfy({ $0.isFinite && $0 >= 0 && $0 <= 16 }),
          ZoenMessageInk.color(fill) != nil else { image.image = nil; image.layer.removeAllAnimations(); gradient.colors = nil; appliedKey = ""; appliedOwner = ""; return }
    let stops = fillColors.isEmpty ? [fill] : fillColors
    let colors = stops.compactMap(ZoenMessageInk.color)
    guard colors.count == stops.count, (1...4).contains(colors.count) else {
      image.image = nil; image.layer.removeAllAnimations(); gradient.colors = nil; appliedKey = ""; appliedOwner = ""; return
    }
    let key = "\(incoming):\(tail):\(raw):\(stops):\(traitCollection.displayScale)"
    guard key != appliedKey || ownerKey != appliedOwner else { return }
    let animate = animateChanges && !UIAccessibility.isReduceMotionEnabled && window != nil && ownerKey == appliedOwner && !appliedKey.isEmpty
    var radii = raw.map { CGFloat($0) }
    if incoming { radii = [radii[1], radii[0], radii[3], radii[2]] }
    let rendered = ZoenMessageInk.bubble(radii: radii, tail: tail, incoming: incoming, color: .white, scale: max(1, traitCollection.displayScale))
    CATransaction.begin(); CATransaction.setDisableActions(true)
    gradient.colors = (colors.count == 1 ? [colors[0], colors[0]] : colors).map(\.cgColor)
    CATransaction.commit()
    if animate, let prior = image.image?.cgImage, let next = rendered.cgImage {
      let animation = CABasicAnimation(keyPath: "contents")
      animation.fromValue = prior; animation.toValue = next; animation.duration = 0.42
      animation.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
      image.layer.add(animation, forKey: "zoen.message.contents")
    } else { image.layer.removeAllAnimations() }
    image.image = rendered; appliedKey = key; appliedOwner = ownerKey
  }
  override func layoutSubviews() {
    super.layoutSubviews(); colorView.frame = bounds; image.frame = colorView.bounds.insetBy(dx: -1, dy: -1)
    CATransaction.begin(); CATransaction.setDisableActions(true); gradient.frame = colorView.bounds.insetBy(dx: -1, dy: -1); CATransaction.commit()
    apply()
  }
  override func didMoveToWindow() { super.didMoveToWindow(); if window == nil { image.layer.removeAllAnimations(); appliedOwner = "" } }
}

@MainActor final class ZoenMessageReceiptView: ExpoView {
  var ownerKey = ""
  var shape = ""
  var color = ""
  var reducedMotion = false
  private let full = UIImageView()
  private let partial = UIImageView()
  private var appliedOwner = ""
  private var appliedShape = ""
  private var appliedColor = ""
  private var appliedScale: CGFloat = 0
  private var presentation = ZoenMessageReceiptPresentation()

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    isUserInteractionEnabled = false; isAccessibilityElement = false; clipsToBounds = false
    for image in [full, partial] { image.isUserInteractionEnabled = false; image.isAccessibilityElement = false; addSubview(image) }
    NotificationCenter.default.addObserver(self, selector: #selector(reduceMotionChanged), name: UIAccessibility.reduceMotionStatusDidChangeNotification, object: nil)
  }
  @objc private func reduceMotionChanged() {
    if UIAccessibility.isReduceMotionEnabled { for image in [full, partial] { image.layer.removeAllAnimations() } }
    apply()
  }
  private func reveal(_ view: UIView) {
    guard !reducedMotion, !UIAccessibility.isReduceMotionEnabled else { return }
    let animation = CABasicAnimation(keyPath: "transform.scale")
    animation.fromValue = 1.3; animation.toValue = 1.0; animation.duration = 0.1
    animation.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
    view.layer.add(animation, forKey: "zoen.message.acknowledgement")
  }
  private func rotate(_ view: UIView, seconds: Double) {
    guard !reducedMotion, !UIAccessibility.isReduceMotionEnabled, view.layer.animation(forKey: "zoen.message.clock") == nil else { return }
    let animation = CABasicAnimation(keyPath: "transform.rotation.z")
    animation.fromValue = 0; animation.toValue = Double.pi * 2; animation.duration = seconds
    animation.timingFunction = CAMediaTimingFunction(name: .linear); animation.repeatCount = .infinity
    animation.beginTime = 1.0
    view.layer.add(animation, forKey: "zoen.message.clock")
  }
  func apply(propsDelivered: Bool = false) {
    if reducedMotion { for image in [full, partial] { image.layer.removeAllAnimations() } }
    guard let selectedShape = ZoenMessageReceiptPresentation.Shape(rawValue: shape), let ink = ZoenMessageInk.color(color) else {
      for image in [full, partial] { image.image = nil; image.layer.removeAllAnimations() }
      presentation.invalidate()
      appliedOwner = ""; appliedShape = ""; appliedColor = ""; return
    }
    let scale = max(1, traitCollection.displayScale)
    let changedOwner = ownerKey.isEmpty || appliedOwner != ownerKey
    let changedShape = appliedShape != shape
    let redraw = changedOwner || changedShape || appliedColor != color || appliedScale != scale
    let motion = presentation.accept(owner: ownerKey, shape: selectedShape, propsDelivered: propsDelivered)
    if motion.resetAnimations { for image in [full, partial] { image.layer.removeAllAnimations() } }
    if redraw {
      full.image = shape == "clock" ? ZoenMessageInk.clock(hand: false, color: ink, scale: scale) : ZoenMessageInk.check(partial: false, color: ink, scale: scale)
      partial.image = shape == "clock" ? ZoenMessageInk.clock(hand: true, color: ink, scale: scale) : ZoenMessageInk.check(partial: true, color: ink, scale: scale)
      partial.isHidden = shape == "single"
    }
    if shape == "clock" {
      full.frame = CGRect(x: 1, y: 2 - 1 / scale, width: 11, height: 11); partial.frame = full.frame
      if window != nil { rotate(full, seconds: 6); rotate(partial, seconds: 1) }
    } else {
      full.frame = CGRect(x: -4, y: 3 - 1 / scale, width: 11, height: 9); partial.frame = CGRect(x: 2, y: 3 - 1 / scale, width: 11, height: 9)
      if motion.revealFull { reveal(full) }
      if motion.revealPartial { reveal(partial) }
    }
    appliedOwner = ownerKey; appliedShape = shape; appliedColor = color; appliedScale = scale
  }
  override func layoutSubviews() { super.layoutSubviews(); apply() }
  override func didMoveToWindow() {
    super.didMoveToWindow()
    presentation.windowChanged(visible: window != nil)
    if window == nil { for image in [full, partial] { image.layer.removeAllAnimations() } }
    else { apply() }
  }
}
