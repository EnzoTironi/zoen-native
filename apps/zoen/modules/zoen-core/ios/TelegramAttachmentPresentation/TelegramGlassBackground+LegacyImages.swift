// Adapted from TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// GPL-2.0-or-later; GPL-3.0 selected. See NOTICE.md and UPSTREAM_SOURCES.json.
import Foundation
import UIKit

public extension GlassBackgroundView {
    static func generateLegacyShadowImage(cornerRadius: CGFloat, shadowInset: CGFloat = 32.0, shadowIntensity: CGFloat = 0.04, shadowBlur: CGFloat = 40.0) -> UIImage? {
        let shadowInnerInset: CGFloat = 0.5
        let diameter = max(1.0, cornerRadius * 2.0)
        let size = CGSize(width: shadowInset * 2.0 + diameter, height: shadowInset * 2.0 + diameter)
        
        return generateImage(size, rotatedContext: { size, context in
            context.clear(CGRect(origin: CGPoint(), size: size))
            
            let shadowRect = CGRect(
                origin: CGPoint(x: shadowInset + shadowInnerInset, y: shadowInset + shadowInnerInset),
                size: CGSize(
                    width: size.width - shadowInset * 2.0 - shadowInnerInset * 2.0,
                    height: size.height - shadowInset * 2.0 - shadowInnerInset * 2.0
                )
            )
            
            context.setFillColor(UIColor.black.cgColor)
            context.setShadow(offset: CGSize(width: 0.0, height: 1.0), blur: shadowBlur, color: UIColor(white: 0.0, alpha: shadowIntensity).cgColor)
            context.fillEllipse(in: shadowRect)
            
            context.setFillColor(UIColor.clear.cgColor)
            context.setBlendMode(.copy)
            context.fillEllipse(in: shadowRect)
        })?.stretchableImage(
            withLeftCapWidth: Int(shadowInset + cornerRadius),
            topCapHeight: Int(shadowInset + cornerRadius)
        )
    }

    static func generateLegacyShadowImage(cornerRadii: CornerRadii, shadowInset: CGFloat = 32.0, shadowIntensity: CGFloat = 0.04, shadowBlur: CGFloat = 40.0) -> UIImage? {
        let shadowInnerInset: CGFloat = 0.5
        let metrics = legacyResizableImageMetrics(cornerRadii: cornerRadii, inset: shadowInset)

        return generateImage(metrics.imageSize, rotatedContext: { _, context in
            context.clear(CGRect(origin: CGPoint(), size: metrics.imageSize))

            let shadowRect = metrics.innerRect.insetBy(dx: shadowInnerInset, dy: shadowInnerInset)
            let shadowPath = GlassBackgroundView.generateRoundedRectPath(rect: shadowRect, cornerRadii: cornerRadii)

            context.setFillColor(UIColor.black.cgColor)
            context.setShadow(offset: CGSize(width: 0.0, height: 1.0), blur: shadowBlur, color: UIColor(white: 0.0, alpha: shadowIntensity).cgColor)
            context.addPath(shadowPath)
            context.fillPath()

            context.setFillColor(UIColor.clear.cgColor)
            context.setBlendMode(.copy)
            context.addPath(shadowPath)
            context.fillPath()
        })?.stretchableImage(
            withLeftCapWidth: metrics.leftCapWidth,
            topCapHeight: metrics.topCapHeight
        )
    }
    
    static func generateLegacyGlassImage(size: CGSize, inset: CGFloat, borderWidthFactor: CGFloat = 1.0, isDark: Bool, fillColor: UIColor) -> UIImage {
        var size = size
        if size == .zero {
            size = CGSize(width: 2.0, height: 2.0)
        }
        let innerSize = size
        size.width += inset * 2.0
        size.height += inset * 2.0
        
        return UIGraphicsImageRenderer(size: size).image { ctx in
            let context = ctx.cgContext
            
            context.clear(CGRect(origin: CGPoint(), size: size))

            let addShadow: (CGContext, Bool, CGPoint, CGFloat, CGFloat, UIColor, CGBlendMode) -> Void = { context, isOuter, position, blur, spread, shadowColor, blendMode in
                var blur = blur
                
                if isOuter {
                    blur += abs(spread)
                    
                    context.beginTransparencyLayer(auxiliaryInfo: nil)
                    context.saveGState()
                    defer {
                        context.restoreGState()
                        context.endTransparencyLayer()
                    }

                    let spreadRect = CGRect(origin: CGPoint(x: inset, y: inset), size: innerSize).insetBy(dx: 0.25, dy: 0.25)
                    let spreadPath = UIBezierPath(
                        roundedRect: spreadRect,
                        cornerRadius: min(spreadRect.width, spreadRect.height) * 0.5
                    ).cgPath

                    context.setShadow(offset: CGSize(width: position.x, height: position.y), blur: blur, color: shadowColor.cgColor)
                    context.setFillColor(UIColor.black.withAlphaComponent(1.0).cgColor)
                    context.addPath(spreadPath)
                    context.fillPath()
                    
                    let cleanRect = CGRect(origin: CGPoint(x: inset, y: inset), size: innerSize)
                    let cleanPath = UIBezierPath(
                        roundedRect: cleanRect,
                        cornerRadius: min(cleanRect.width, cleanRect.height) * 0.5
                    ).cgPath
                    context.setBlendMode(.copy)
                    context.setFillColor(UIColor.clear.cgColor)
                    context.addPath(cleanPath)
                    context.fillPath()
                    context.setBlendMode(.normal)
                } else {
                    let image = UIGraphicsImageRenderer(size: size).image(actions: { ctx in
                        let context = ctx.cgContext
                        
                        context.clear(CGRect(origin: CGPoint(), size: size))
                        let spreadRect = CGRect(origin: CGPoint(x: inset, y: inset), size: innerSize).insetBy(dx: -spread - 0.33, dy: -spread - 0.33)

                        context.setShadow(offset: CGSize(width: position.x, height: position.y), blur: blur, color: shadowColor.cgColor)
                        context.setFillColor(shadowColor.cgColor)
                        let enclosingRect = spreadRect.insetBy(dx: -10000.0, dy: -10000.0)
                        context.addPath(UIBezierPath(rect: enclosingRect).cgPath)
                        context.addBadgePath(in: spreadRect)
                        context.fillPath(using: .evenOdd)
                    })
                    
                    UIGraphicsPushContext(context)
                    image.draw(in: CGRect(origin: .zero, size: size), blendMode: blendMode, alpha: 1.0)
                    UIGraphicsPopContext()
                }
            }
            
            addShadow(context, true, CGPoint(), 30.0, 0.0, UIColor(white: 0.0, alpha: 0.045), .normal)
            addShadow(context, true, CGPoint(), 20.0, 0.0, UIColor(white: 0.0, alpha: 0.01), .normal)
            
            var a: CGFloat = 0.0
            var b: CGFloat = 0.0
            var s: CGFloat = 0.0
            fillColor.getHue(nil, saturation: &s, brightness: &b, alpha: &a)
            
            let innerImage: UIImage
            /*if size == CGSize(width: 40.0 + inset * 2.0, height: 40.0 + inset * 2.0), b >= 0.2 {
                innerImage = UIGraphicsImageRenderer(size: size).image { ctx in
                    let context = ctx.cgContext
                    
                    context.setFillColor(fillColor.cgColor)
                    context.fill(CGRect(origin: CGPoint(), size: size))
                    
                    if let image = UIImage(bundleImageName: "Item List/GlassEdge40x40") {
                        let imageInset = (image.size.width - 40.0) * 0.5
                        
                        if s == 0.0 && abs(a - 0.7) < 0.1 && !isDark {
                            image.draw(in: CGRect(origin: CGPoint(), size: size).insetBy(dx: inset - imageInset, dy: inset - imageInset), blendMode: .normal, alpha: 1.0)
                        } else if s <= 0.3 && !isDark {
                            image.draw(in: CGRect(origin: CGPoint(), size: size).insetBy(dx: inset - imageInset, dy: inset - imageInset), blendMode: .normal, alpha: 0.7)
                        } else if b >= 0.2 {
                            let maxAlpha: CGFloat = isDark ? 0.7 : 0.8
                            image.draw(in: CGRect(origin: CGPoint(), size: size).insetBy(dx: inset - imageInset, dy: inset - imageInset), blendMode: .overlay, alpha: max(0.5, min(1.0, maxAlpha * s)))
                        } else {
                            image.draw(in: CGRect(origin: CGPoint(), size: size).insetBy(dx: inset - imageInset, dy: inset - imageInset), blendMode: .normal, alpha: 0.5)
                        }
                    }
                }
            } else {
                innerImage = UIGraphicsImageRenderer(size: size).image { ctx in
                    let context = ctx.cgContext
                    
                    context.setFillColor(fillColor.cgColor)
                    context.fill(CGRect(origin: CGPoint(), size: size).insetBy(dx: inset, dy: inset).insetBy(dx: 0.1, dy: 0.1))
                    
                    addShadow(context, true, CGPoint(x: 0.0, y: 0.0), 20.0, 0.0, UIColor(white: 0.0, alpha: 0.04), .normal)
                    addShadow(context, true, CGPoint(x: 0.0, y: 0.0), 5.0, 0.0, UIColor(white: 0.0, alpha: 0.04), .normal)
                    
                    if s <= 0.3 && !isDark {
                        addShadow(context, false, CGPoint(x: 0.0, y: 0.0), 8.0, 0.0, UIColor(white: 0.0, alpha: 0.4), .overlay)
                        
                        let edgeAlpha: CGFloat = max(0.8, min(1.0, a))
                        
                        for _ in 0 ..< 2 {
                            addShadow(context, false, CGPoint(x: -0.64, y: -0.64), 0.8, 0.0, UIColor(white: 1.0, alpha: edgeAlpha), .normal)
                            addShadow(context, false, CGPoint(x: 0.64, y: 0.64), 0.8, 0.0, UIColor(white: 1.0, alpha: edgeAlpha), .normal)
                        }
                    } else if b >= 0.2 {
                        let edgeAlpha: CGFloat = max(0.2, min(isDark ? 0.5 : 0.7, a * a * a))
                        
                        addShadow(context, false, CGPoint(x: -0.64, y: -0.64), 0.5, 0.0, UIColor(white: 1.0, alpha: edgeAlpha), .plusLighter)
                        addShadow(context, false, CGPoint(x: 0.64, y: 0.64), 0.5, 0.0, UIColor(white: 1.0, alpha: edgeAlpha), .plusLighter)
                    } else {
                        let edgeAlpha: CGFloat = max(0.4, min(isDark ? 0.5 : 0.7, a * a * a))
                        
                        addShadow(context, false, CGPoint(x: -0.64, y: -0.64), 1.2, 0.0, UIColor(white: 1.0, alpha: edgeAlpha), .normal)
                        addShadow(context, false, CGPoint(x: 0.64, y: 0.64), 1.2, 0.0, UIColor(white: 1.0, alpha: edgeAlpha), .normal)
                    }
                }
            }*/
            
            innerImage = UIGraphicsImageRenderer(size: size).image { ctx in
                let context = ctx.cgContext
                
                context.setFillColor(fillColor.cgColor)
                var ellipseRect = CGRect(origin: CGPoint(), size: size).insetBy(dx: inset, dy: inset)
                context.fillEllipse(in: ellipseRect)
                
                let lineWidth: CGFloat = (isDark ? 0.8 : 0.8) * borderWidthFactor
                let strokeColor: UIColor
                let blendMode: CGBlendMode
                let baseAlpha: CGFloat = isDark ? 0.3 : 0.6
                
                if s == 0.0 && abs(a - 0.7) < 0.1 && !isDark {
                    blendMode = .normal
                    strokeColor = UIColor(white: 1.0, alpha: baseAlpha)
                } else if s <= 0.3 && !isDark {
                    blendMode = .normal
                    strokeColor = UIColor(white: 1.0, alpha: 0.7 * baseAlpha)
                } else if b >= 0.2 {
                    let maxAlpha: CGFloat = isDark ? 0.7 : 0.8
                    blendMode = .overlay
                    strokeColor = UIColor(white: 1.0, alpha: max(0.5, min(1.0, maxAlpha * s)) * baseAlpha)
                } else {
                    blendMode = .normal
                    strokeColor = UIColor(white: 1.0, alpha: 0.5 * baseAlpha)
                }
                
                context.setStrokeColor(strokeColor.cgColor)
                ellipseRect = CGRect(origin: CGPoint(), size: size).insetBy(dx: inset, dy: inset)
                context.addEllipse(in: ellipseRect)
                context.clip()
                
                ellipseRect = CGRect(origin: CGPoint(), size: size).insetBy(dx: inset, dy: inset).insetBy(dx: lineWidth * 0.5, dy: lineWidth * 0.5)
                
                context.setBlendMode(blendMode)
                
                let radius = ellipseRect.height * 0.5
                let smallerRadius = radius - lineWidth * 1.33
                context.move(to: CGPoint(x: ellipseRect.minX, y: ellipseRect.minY + radius))
                // Top-left corner (regular radius)
                context.addArc(tangent1End: CGPoint(x: ellipseRect.minX, y: ellipseRect.minY), tangent2End: CGPoint(x: ellipseRect.minX + radius, y: ellipseRect.minY), radius: radius)
                context.addLine(to: CGPoint(x: ellipseRect.maxX - smallerRadius, y: ellipseRect.minY))
                // Top-right corner (smaller radius)
                context.addArc(tangent1End: CGPoint(x: ellipseRect.maxX, y: ellipseRect.minY), tangent2End: CGPoint(x: ellipseRect.maxX, y: ellipseRect.minY + smallerRadius), radius: smallerRadius)
                context.addLine(to: CGPoint(x: ellipseRect.maxX, y: ellipseRect.maxY - radius))
                // Bottom-right corner (regular radius)
                context.addArc(tangent1End: CGPoint(x: ellipseRect.maxX, y: ellipseRect.maxY), tangent2End: CGPoint(x: ellipseRect.maxX - radius, y: ellipseRect.maxY), radius: radius)
                context.addLine(to: CGPoint(x: ellipseRect.minX + smallerRadius, y: ellipseRect.maxY))
                // Bottom-left corner (smaller radius)
                context.addArc(tangent1End: CGPoint(x: ellipseRect.minX, y: ellipseRect.maxY), tangent2End: CGPoint(x: ellipseRect.minX, y: ellipseRect.maxY - smallerRadius), radius: smallerRadius)
                context.closePath()
                context.strokePath()
                
                context.resetClip()
                context.setBlendMode(.normal)
            }
            innerImage.draw(in: CGRect(origin: CGPoint(), size: size))
        }.stretchableImage(withLeftCapWidth: Int(size.width * 0.5), topCapHeight: Int(size.height * 0.5))
    }
    
    static func generateLegacyGlassImage(cornerRadii: CornerRadii, inset: CGFloat, borderWidthFactor: CGFloat = 1.0, isDark: Bool, fillColor: UIColor) -> UIImage {
        let metrics = legacyResizableImageMetrics(cornerRadii: cornerRadii, inset: inset)
        let size = metrics.imageSize
        let innerRect = metrics.innerRect

        return UIGraphicsImageRenderer(size: size).image { ctx in
            let context = ctx.cgContext

            context.clear(CGRect(origin: CGPoint(), size: size))

            let addShadow: (CGContext, Bool, CGPoint, CGFloat, CGFloat, UIColor, CGBlendMode) -> Void = { context, isOuter, position, blur, spread, shadowColor, blendMode in
                var blur = blur

                if isOuter {
                    blur += abs(spread)

                    context.beginTransparencyLayer(auxiliaryInfo: nil)
                    context.saveGState()
                    defer {
                        context.restoreGState()
                        context.endTransparencyLayer()
                    }

                    let spreadRect = innerRect.insetBy(dx: 0.25, dy: 0.25)
                    let spreadPath = GlassBackgroundView.generateRoundedRectPath(rect: spreadRect, cornerRadii: cornerRadii)

                    context.setShadow(offset: CGSize(width: position.x, height: position.y), blur: blur, color: shadowColor.cgColor)
                    context.setFillColor(UIColor.black.withAlphaComponent(1.0).cgColor)
                    context.addPath(spreadPath)
                    context.fillPath()

                    let cleanPath = GlassBackgroundView.generateRoundedRectPath(rect: innerRect, cornerRadii: cornerRadii)
                    context.setBlendMode(.copy)
                    context.setFillColor(UIColor.clear.cgColor)
                    context.addPath(cleanPath)
                    context.fillPath()
                    context.setBlendMode(.normal)
                } else {
                    let image = UIGraphicsImageRenderer(size: size).image(actions: { ctx in
                        let context = ctx.cgContext
                        let spreadRect = innerRect.insetBy(dx: -spread - 0.33, dy: -spread - 0.33)

                        context.clear(CGRect(origin: CGPoint(), size: size))
                        context.setShadow(offset: CGSize(width: position.x, height: position.y), blur: blur, color: shadowColor.cgColor)
                        context.setFillColor(shadowColor.cgColor)
                        context.addPath(UIBezierPath(rect: spreadRect.insetBy(dx: -10000.0, dy: -10000.0)).cgPath)
                        context.addPath(GlassBackgroundView.generateRoundedRectPath(rect: spreadRect, cornerRadii: cornerRadii))
                        context.fillPath(using: .evenOdd)
                    })

                    UIGraphicsPushContext(context)
                    image.draw(in: CGRect(origin: .zero, size: size), blendMode: blendMode, alpha: 1.0)
                    UIGraphicsPopContext()
                }
            }

            addShadow(context, true, CGPoint(), 30.0, 0.0, UIColor(white: 0.0, alpha: 0.045), .normal)
            addShadow(context, true, CGPoint(), 20.0, 0.0, UIColor(white: 0.0, alpha: 0.01), .normal)

            var a: CGFloat = 0.0
            var b: CGFloat = 0.0
            var s: CGFloat = 0.0
            fillColor.getHue(nil, saturation: &s, brightness: &b, alpha: &a)

            context.setFillColor(fillColor.cgColor)
            context.addPath(GlassBackgroundView.generateRoundedRectPath(rect: innerRect, cornerRadii: cornerRadii))
            context.fillPath()

            let lineWidth: CGFloat = (isDark ? 0.8 : 0.8) * borderWidthFactor
            let strokeColor: UIColor
            let blendMode: CGBlendMode
            let baseAlpha: CGFloat = isDark ? 0.3 : 0.6

            if s == 0.0 && abs(a - 0.7) < 0.1 && !isDark {
                blendMode = .normal
                strokeColor = UIColor(white: 1.0, alpha: baseAlpha)
            } else if s <= 0.3 && !isDark {
                blendMode = .normal
                strokeColor = UIColor(white: 1.0, alpha: 0.7 * baseAlpha)
            } else if b >= 0.2 {
                let maxAlpha: CGFloat = isDark ? 0.7 : 0.8
                blendMode = .overlay
                strokeColor = UIColor(white: 1.0, alpha: max(0.5, min(1.0, maxAlpha * s)) * baseAlpha)
            } else {
                blendMode = .normal
                strokeColor = UIColor(white: 1.0, alpha: 0.5 * baseAlpha)
            }

            context.addPath(GlassBackgroundView.generateRoundedRectPath(rect: innerRect, cornerRadii: cornerRadii))
            context.clip()
            context.setBlendMode(blendMode)
            context.setLineWidth(lineWidth)
            context.setStrokeColor(strokeColor.cgColor)
            context.addPath(GlassBackgroundView.generateRoundedRectPath(rect: innerRect.insetBy(dx: lineWidth * 0.5, dy: lineWidth * 0.5), cornerRadii: cornerRadii.insetBy(lineWidth * 0.5)))
            context.strokePath()
            context.resetClip()
            context.setBlendMode(.normal)
        }.stretchableImage(withLeftCapWidth: metrics.leftCapWidth, topCapHeight: metrics.topCapHeight)
    }

}
