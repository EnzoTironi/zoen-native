// Adapted from TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// GPL-2.0-or-later; GPL-3.0 selected. See NOTICE.md and UPSTREAM_SOURCES.json.
import Foundation
import UIKit

extension GlassBackgroundView {
    static func clampedCornerRadii(size: CGSize, cornerRadii: CornerRadii) -> CornerRadii {
        let size = CGSize(width: max(0.0, size.width), height: max(0.0, size.height))
        var cornerRadii = CornerRadii(
            topLeft: max(0.0, cornerRadii.topLeft),
            topRight: max(0.0, cornerRadii.topRight),
            bottomLeft: max(0.0, cornerRadii.bottomLeft),
            bottomRight: max(0.0, cornerRadii.bottomRight)
        )

        func scaleFor(edgeLength: CGFloat, _ lhs: CGFloat, _ rhs: CGFloat) -> CGFloat {
            let sum = lhs + rhs
            if sum <= edgeLength || sum.isZero {
                return 1.0
            }
            return edgeLength / sum
        }

        let scale = min(
            1.0,
            scaleFor(edgeLength: size.width, cornerRadii.topLeft, cornerRadii.topRight),
            scaleFor(edgeLength: size.width, cornerRadii.bottomLeft, cornerRadii.bottomRight),
            scaleFor(edgeLength: size.height, cornerRadii.topLeft, cornerRadii.bottomLeft),
            scaleFor(edgeLength: size.height, cornerRadii.topRight, cornerRadii.bottomRight)
        )

        if scale < 1.0 {
            cornerRadii = CornerRadii(
                topLeft: cornerRadii.topLeft * scale,
                topRight: cornerRadii.topRight * scale,
                bottomLeft: cornerRadii.bottomLeft * scale,
                bottomRight: cornerRadii.bottomRight * scale
            )
        }

        return cornerRadii
    }

    static func generateRoundedRectPath(rect: CGRect, cornerRadii: CornerRadii) -> CGPath {
        let cornerRadii = self.clampedCornerRadii(size: rect.size, cornerRadii: cornerRadii)
        let path = CGMutablePath()

        func addCorner(tangent1End: CGPoint, tangent2End: CGPoint, radius: CGFloat) {
            if radius > CGFloat.ulpOfOne {
                path.addArc(tangent1End: tangent1End, tangent2End: tangent2End, radius: radius)
            } else {
                path.addLine(to: tangent1End)
                path.addLine(to: tangent2End)
            }
        }

        path.move(to: CGPoint(x: rect.minX, y: rect.minY + cornerRadii.topLeft))
        addCorner(
            tangent1End: CGPoint(x: rect.minX, y: rect.minY),
            tangent2End: CGPoint(x: rect.minX + cornerRadii.topLeft, y: rect.minY),
            radius: cornerRadii.topLeft
        )
        path.addLine(to: CGPoint(x: rect.maxX - cornerRadii.topRight, y: rect.minY))
        addCorner(
            tangent1End: CGPoint(x: rect.maxX, y: rect.minY),
            tangent2End: CGPoint(x: rect.maxX, y: rect.minY + cornerRadii.topRight),
            radius: cornerRadii.topRight
        )
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY - cornerRadii.bottomRight))
        addCorner(
            tangent1End: CGPoint(x: rect.maxX, y: rect.maxY),
            tangent2End: CGPoint(x: rect.maxX - cornerRadii.bottomRight, y: rect.maxY),
            radius: cornerRadii.bottomRight
        )
        path.addLine(to: CGPoint(x: rect.minX + cornerRadii.bottomLeft, y: rect.maxY))
        addCorner(
            tangent1End: CGPoint(x: rect.minX, y: rect.maxY),
            tangent2End: CGPoint(x: rect.minX, y: rect.maxY - cornerRadii.bottomLeft),
            radius: cornerRadii.bottomLeft
        )
        path.closeSubpath()

        return path
    }

    static func generateRoundedRectPath(size: CGSize, cornerRadii: CornerRadii) -> CGPath {
        return self.generateRoundedRectPath(rect: CGRect(origin: CGPoint(), size: size), cornerRadii: cornerRadii)
    }
    
    final class ClippingShapeContext {
        let view: UIView
        private var maskLayer: CAShapeLayer?
        
        private(set) var shape: Shape?
        
        init(view: UIView) {
            self.view = view
        }
        
        func update(shape: Shape, size: CGSize, transition: ComponentTransition) {
            self.shape = shape
            
            switch shape {
            case let .roundedRect(cornerRadius):
                self.maskLayer = nil
                self.view.layer.mask = nil
                transition.setCornerRadius(layer: self.view.layer, cornerRadius: cornerRadius)
            case let .customRoundedRect(cornerRadii):
                transition.setCornerRadius(layer: self.view.layer, cornerRadius: 0.0)
                if #available(iOS 26.0, *) {
                    transition.animateView {
                        self.view.cornerConfiguration = .corners(
                            topLeftRadius: .fixed(cornerRadii.topLeft),
                            topRightRadius: .fixed(cornerRadii.topRight),
                            bottomLeftRadius: .fixed(cornerRadii.bottomLeft),
                            bottomRightRadius: .fixed(cornerRadii.bottomRight)
                        )
                    }
                } else {
                    let maskLayer: CAShapeLayer
                    if let current = self.maskLayer {
                        maskLayer = current
                    } else {
                        maskLayer = CAShapeLayer()
                        maskLayer.fillColor = UIColor.black.cgColor
                        self.maskLayer = maskLayer
                        self.view.layer.mask = maskLayer
                    }
                    transition.setFrame(layer: maskLayer, frame: CGRect(origin: CGPoint(), size: size))
                    transition.setShapeLayerPath(layer: maskLayer, path: GlassBackgroundView.generateRoundedRectPath(size: size, cornerRadii: cornerRadii))
                }
            }
        }
    }
    
}
