// Adapted from TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// GPL-2.0-or-later; GPL-3.0 selected. See NOTICE.md and UPSTREAM_SOURCES.json.
import Foundation
import UIKit

extension CGContext {
    func addBadgePath(in rect: CGRect) {
        saveGState()
        translateBy(x: rect.minX, y: rect.minY)
        scaleBy(x: rect.width / 78.0, y: rect.height / 78.0)
        
        // M 0 39
        move(to: CGPoint(x: 0, y: 39))
        
        // C 0 17.4609 17.4609 0 39 0
        addCurve(to: CGPoint(x: 39, y: 0),
                 control1: CGPoint(x: 0,       y: 17.4609),
                 control2: CGPoint(x: 17.4609, y: 0))
        
        // H 42
        addLine(to: CGPoint(x: 42, y: 0))
        
        // C 61.8823 0 78 16.1177 78 36
        addCurve(to: CGPoint(x: 78, y: 36),
                 control1: CGPoint(x: 61.8823, y: 0),
                 control2: CGPoint(x: 78,      y: 16.1177))
        
        // V 39
        addLine(to: CGPoint(x: 78, y: 39))
        
        // C 78 60.5391 60.5391 78 39 78
        addCurve(to: CGPoint(x: 39, y: 78),
                 control1: CGPoint(x: 78,      y: 60.5391),
                 control2: CGPoint(x: 60.5391, y: 78))
        
        // H 36
        addLine(to: CGPoint(x: 36, y: 78))
        
        // C 16.1177 78 0 61.8823 0 42
        addCurve(to: CGPoint(x: 0, y: 42),
                 control1: CGPoint(x: 16.1177, y: 78),
                 control2: CGPoint(x: 0,       y: 61.8823))
        
        // V 39 / Z
        addLine(to: CGPoint(x: 0, y: 39))
        closePath()
        
        restoreGState()
    }
}

struct LegacyResizableImageMetrics {
    let imageSize: CGSize
    let innerRect: CGRect
    let leftCapWidth: Int
    let topCapHeight: Int
}

func legacyResizableImageMetrics(cornerRadii: GlassBackgroundView.CornerRadii, inset: CGFloat) -> LegacyResizableImageMetrics {
    let leftRadius = ceil(max(cornerRadii.topLeft, cornerRadii.bottomLeft))
    let rightRadius = ceil(max(cornerRadii.topRight, cornerRadii.bottomRight))
    let topRadius = ceil(max(cornerRadii.topLeft, cornerRadii.topRight))
    let bottomRadius = ceil(max(cornerRadii.bottomLeft, cornerRadii.bottomRight))

    let innerSize = CGSize(
        width: max(1.0, leftRadius + rightRadius + 1.0),
        height: max(1.0, topRadius + bottomRadius + 1.0)
    )
    let imageSize = CGSize(width: innerSize.width + inset * 2.0, height: innerSize.height + inset * 2.0)

    return LegacyResizableImageMetrics(
        imageSize: imageSize,
        innerRect: CGRect(origin: CGPoint(x: inset, y: inset), size: innerSize),
        leftCapWidth: Int(ceil(inset + leftRadius)),
        topCapHeight: Int(ceil(inset + topRadius))
    )
}


public extension GlassBackgroundView {
    static func generateForegroundImage(size: CGSize, isDark: Bool, fillColor: UIColor) -> UIImage {
        var size = size
        if size == .zero {
            size = CGSize(width: 1.0, height: 1.0)
        }
        
        return generateImage(size, rotatedContext: { size, context in
            context.clear(CGRect(origin: CGPoint(), size: size))
            
            let maxColor = UIColor(white: 1.0, alpha: isDark ? 0.2 : 0.9)
            let minColor = UIColor(white: 1.0, alpha: 0.0)
            
            context.setFillColor(fillColor.cgColor)
            context.fillEllipse(in: CGRect(origin: CGPoint(), size: size))
            
            let lineWidth: CGFloat = isDark ? 0.33 : 0.66
            
            context.saveGState()
            
            let darkShadeColor = UIColor(white: isDark ? 1.0 : 0.0, alpha: isDark ? 0.0 : 0.035)
            let lightShadeColor = UIColor(white: isDark ? 0.0 : 1.0, alpha: isDark ? 0.0 : 0.035)
            let innerShadowBlur: CGFloat = 24.0
            
            context.resetClip()
            context.addEllipse(in: CGRect(origin: CGPoint(), size: size).insetBy(dx: lineWidth * 0.5, dy: lineWidth * 0.5))
            context.clip()
            context.addRect(CGRect(origin: CGPoint(), size: size).insetBy(dx: -100.0, dy: -100.0))
            context.addEllipse(in: CGRect(origin: CGPoint(), size: size))
            context.setFillColor(UIColor.black.cgColor)
            context.setShadow(offset: CGSize(width: 10.0, height: -10.0), blur: innerShadowBlur, color: darkShadeColor.cgColor)
            context.fillPath(using: .evenOdd)
            
            context.resetClip()
            context.addEllipse(in: CGRect(origin: CGPoint(), size: size).insetBy(dx: lineWidth * 0.5, dy: lineWidth * 0.5))
            context.clip()
            context.addRect(CGRect(origin: CGPoint(), size: size).insetBy(dx: -100.0, dy: -100.0))
            context.addEllipse(in: CGRect(origin: CGPoint(), size: size))
            context.setFillColor(UIColor.black.cgColor)
            context.setShadow(offset: CGSize(width: -10.0, height: 10.0), blur: innerShadowBlur, color: lightShadeColor.cgColor)
            context.fillPath(using: .evenOdd)
            
            context.restoreGState()
            
            context.setLineWidth(lineWidth)
            
            context.addRect(CGRect(origin: CGPoint(x: 0.0, y: 0.0), size: CGSize(width: size.width * 0.5, height: size.height)))
            context.clip()
            context.addEllipse(in: CGRect(origin: CGPoint(), size: size).insetBy(dx: lineWidth * 0.5, dy: lineWidth * 0.5))
            context.replacePathWithStrokedPath()
            context.clip()
            
            do {
                var locations: [CGFloat] = [0.0, 0.5, 0.5 + 0.2, 1.0 - 0.1, 1.0]
                let colors: [CGColor] = [maxColor.cgColor, maxColor.cgColor, minColor.cgColor, minColor.cgColor, maxColor.cgColor]
                
                let colorSpace = CGColorSpaceCreateDeviceRGB()
                let gradient = CGGradient(colorsSpace: colorSpace, colors: colors as CFArray, locations: &locations)!
                
                context.drawLinearGradient(gradient, start: CGPoint(x: 0.0, y: 0.0), end: CGPoint(x: 0.0, y: size.height), options: CGGradientDrawingOptions())
            }
            
            context.resetClip()
            context.addRect(CGRect(origin: CGPoint(x: size.width - size.width * 0.5, y: 0.0), size: CGSize(width: size.width * 0.5, height: size.height)))
            context.clip()
            context.addEllipse(in: CGRect(origin: CGPoint(), size: size).insetBy(dx: lineWidth * 0.5, dy: lineWidth * 0.5))
            context.replacePathWithStrokedPath()
            context.clip()
            
            do {
                var locations: [CGFloat] = [0.0, 0.1, 0.5 - 0.2, 0.5, 1.0]
                let colors: [CGColor] = [maxColor.cgColor, minColor.cgColor, minColor.cgColor, maxColor.cgColor, maxColor.cgColor]
                
                let colorSpace = CGColorSpaceCreateDeviceRGB()
                let gradient = CGGradient(colorsSpace: colorSpace, colors: colors as CFArray, locations: &locations)!
                
                context.drawLinearGradient(gradient, start: CGPoint(x: 0.0, y: 0.0), end: CGPoint(x: 0.0, y: size.height), options: CGGradientDrawingOptions())
            }
        })!.stretchableImage(withLeftCapWidth: Int(size.width * 0.5), topCapHeight: Int(size.height * 0.5))
    }
}
