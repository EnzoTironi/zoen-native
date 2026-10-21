// Local module boundary for the pinned Telegram rendering closure.
// Upstream method bodies are preserved where noted in UPSTREAM_SOURCES.json.
import Foundation
import UIKit
import Darwin

public let kCAOnOrderIn = "onOrderIn"
public let kCAOnOrderOut = "onOrderOut"

// Display/DeviceMetrics.swift:9–18. Only its renderer capability predicate is
// needed; no model catalogue, account/session or networking dependencies.
enum DeviceMetrics {
  struct Performance {
    let isGraphicallyCapable: Bool
    init() {
      var length = 4
      var cpuCount: UInt32 = 0
      sysctlbyname("hw.ncpu", &cpuCount, &length, nil, 0)
      isGraphicallyCapable = cpuCount >= 4
    }
  }
  static let performance = Performance()
}

public func makeSpringAnimation(_ keyPath: String, duration: Double) -> CABasicAnimation {
  makeSpringAnimationImpl(keyPath, duration)
}
public func makeSpringBounceAnimation(_ keyPath: String, _ initialVelocity: CGFloat,
                                     _ damping: CGFloat) -> CASpringAnimation {
  makeSpringBounceAnimationImpl(keyPath, initialVelocity, damping)
}
public extension CALayer {
  static func blur() -> NSObject? { makeBlurFilter() }
  static func luminanceToAlpha() -> NSObject? { makeLuminanceToAlphaFilter() }
  static func colorMatrix() -> NSObject? { makeColorMatrixFilter() }
}

// Display/UIKitUtils.swift:990–1012, with the original guarded ObjC rendering
// treatment in ZoenTelegramPresentationRuntime.m.
public extension UIView {
  func setMonochromaticEffect(tintColor: UIColor?) {
    var overrideUserInterfaceStyle: UIUserInterfaceStyle = .unspecified
    var red: CGFloat = 0.0
    var green: CGFloat = 0.0
    var blue: CGFloat = 0.0
    var alpha: CGFloat = 1.0
    if let tintColor {
      if tintColor.getRed(&red, green: &green, blue: &blue, alpha: &alpha) {
        if red == 0.0 && green == 0.0 && blue == 0.0 && alpha == 1.0 {
          overrideUserInterfaceStyle = .light
        }
      } else {
        if red == 1.0 && green == 1.0 && blue == 1.0 && alpha == 1.0 {
          overrideUserInterfaceStyle = .dark
        }
      }
    }
    if self.overrideUserInterfaceStyle != overrideUserInterfaceStyle {
      self.overrideUserInterfaceStyle = overrideUserInterfaceStyle
      setMonochromaticEffectImpl(self, overrideUserInterfaceStyle != .unspecified)
    }
  }
}

// Display image contexts are adapted to UIKit's native renderer rather than
// importing AsyncDisplayKit. The pinned glass path/circle/shadow drawing remains
// unchanged; this adapter is used only by the upstream legacy glass fallback.
public func generateImage(_ size: CGSize, contextGenerator: (CGSize, CGContext) -> Void,
                          opaque: Bool = false, scale: CGFloat? = nil) -> UIImage? {
  guard size.width > 0, size.height > 0 else { return nil }
  let format = UIGraphicsImageRendererFormat()
  format.opaque = opaque
  if let scale, scale > 0 { format.scale = scale }
  return UIGraphicsImageRenderer(size: size, format: format).image { renderer in
    contextGenerator(size, renderer.cgContext)
  }
}
public func generateImage(_ size: CGSize, opaque: Bool = false, scale: CGFloat? = nil,
                          rotatedContext: (CGSize, CGContext) -> Void) -> UIImage? {
  generateImage(size, contextGenerator: { size, context in
    context.translateBy(x: 0, y: size.height)
    context.scaleBy(x: 1, y: -1)
    rotatedContext(size, context)
  }, opaque: opaque, scale: scale)
}

public extension UIColor {
  func mixedWith(_ color: UIColor, alpha: CGFloat) -> UIColor {
    var r1: CGFloat = 0, g1: CGFloat = 0, b1: CGFloat = 0, a1: CGFloat = 0
    var r2: CGFloat = 0, g2: CGFloat = 0, b2: CGFloat = 0, a2: CGFloat = 0
    getRed(&r1, green: &g1, blue: &b1, alpha: &a1)
    color.getRed(&r2, green: &g2, blue: &b2, alpha: &a2)
    let inverse = 1 - alpha
    return UIColor(red: r1 * inverse + r2 * alpha, green: g1 * inverse + g2 * alpha,
                   blue: b1 * inverse + b2 * alpha, alpha: a1 * inverse + a2 * alpha)
  }
}

// Display/GenerateImage.swift:128–159,237–252; original geometry/cap widths.
public func generateFilledCircleImage(diameter: CGFloat, color: UIColor?, strokeColor: UIColor? = nil, strokeWidth: CGFloat? = nil, backgroundColor: UIColor? = nil) -> UIImage? {
    return generateImage(CGSize(width: diameter, height: diameter), contextGenerator: { size, context in
        context.clear(CGRect(origin: CGPoint(), size: size))
        if let backgroundColor = backgroundColor {
            context.setFillColor(backgroundColor.cgColor)
            context.fill(CGRect(origin: CGPoint(), size: size))
        }
        
        if let strokeColor = strokeColor, let strokeWidth = strokeWidth {
            context.setFillColor(strokeColor.cgColor)
            context.fillEllipse(in: CGRect(origin: CGPoint(), size: size))
            
            if let color = color {
                context.setFillColor(color.cgColor)
            } else {
                context.setFillColor(UIColor.clear.cgColor)
                context.setBlendMode(.copy)
            }
            context.fillEllipse(in: CGRect(origin: CGPoint(x: strokeWidth, y: strokeWidth), size: CGSize(width: size.width - strokeWidth * 2.0, height: size.height - strokeWidth * 2.0)))
        } else {
            if let color = color {
                context.setFillColor(color.cgColor)
            } else {
                context.setFillColor(UIColor.clear.cgColor)
                context.setBlendMode(.copy)
            }
            context.fillEllipse(in: CGRect(origin: CGPoint(), size: size))
        }
    })
}
public func generateStretchableFilledCircleImage(diameter: CGFloat, color: UIColor?, strokeColor: UIColor? = nil, strokeWidth: CGFloat? = nil, backgroundColor: UIColor? = nil) -> UIImage? {
    let intRadius = Int(diameter / 2.0)
    let intDiameter = Int(diameter)
    let cap: Int
    if intDiameter == 3 {
        cap = 1
    } else if intDiameter == 2 {
        cap = 3
    } else if intRadius == 1 {
        cap = 2
    } else {
        cap = intRadius
    }
    
    return generateFilledCircleImage(diameter: diameter, color: color, strokeColor: strokeColor, strokeWidth: strokeWidth, backgroundColor: backgroundColor)?.stretchableImage(withLeftCapWidth: cap, topCapHeight: cap)
}
