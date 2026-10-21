// Adapted from TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// GPL-2.0-or-later; GPL-3.0 selected. See NOTICE.md and UPSTREAM_SOURCES.json.
import Foundation
import UIKit

#if targetEnvironment(simulator)
@_silgen_name("UIAnimationDragCoefficient") func UIAnimationDragCoefficient() -> Float
#endif

public extension UIView {
    static var animationDurationFactor: Double {
        #if targetEnvironment(simulator)
        return Double(UIAnimationDragCoefficient())
        #else
        return 1.0
        #endif
    }
}

public extension CALayer {
    func animate(from: Any, to: Any, keyPath: String, duration: Double, delay: Double, curve: ComponentTransition.Animation.Curve, removeOnCompletion: Bool, additive: Bool, completion: ((Bool) -> Void)? = nil, key: String? = nil) {
        if case let .bounce(stiffness, damping) = curve {
            self.animateSpring(
                from: from,
                to: to,
                keyPath: keyPath,
                duration: duration,
                delay: delay,
                stiffness: stiffness,
                damping: damping,
                removeOnCompletion: removeOnCompletion,
                additive: additive,
                completion: completion,
                key: key
            )
        } else {
            let timingFunction: String
            let mediaTimingFunction: CAMediaTimingFunction?
            switch curve {
            case .spring:
                timingFunction = kCAMediaTimingFunctionSpring
                mediaTimingFunction = nil
            default:
                timingFunction = CAMediaTimingFunctionName.easeInEaseOut.rawValue
                mediaTimingFunction = curve.asTimingFunction()
            }
            
            self.animate(
                from: from,
                to: to,
                keyPath: keyPath,
                timingFunction: timingFunction,
                duration: duration,
                delay: delay,
                mediaTimingFunction: mediaTimingFunction,
                removeOnCompletion: removeOnCompletion,
                additive: additive,
                completion: completion,
                key: key
            )
        }
    }
}

private extension ComponentTransition.Animation.Curve {
    func asTimingFunction() -> CAMediaTimingFunction {
        switch self {
        case .easeInOut:
            return CAMediaTimingFunction(name: .easeInEaseOut)
        case .easeIn:
            return CAMediaTimingFunction(name: .easeIn)
        case .linear:
            return CAMediaTimingFunction(name: .linear)
        case let .custom(a, b, c, d):
            return CAMediaTimingFunction(controlPoints: a, b, c, d)
        case .spring, .bounce:
            preconditionFailure()
        }
    }

    var viewAnimationOptions: UIView.AnimationOptions {
        switch self {
        case .linear:
            return [.curveLinear]
        case .easeInOut:
            return [.curveEaseInOut]
        case .easeIn:
            return [.curveEaseIn]
        case .spring:
            return UIView.AnimationOptions(rawValue: 7 << 16)
        case .custom:
            return []
        case .bounce:
            return []
        }
    }
}

public extension ComponentTransition.Animation {
    var isImmediate: Bool {
        if case .none = self {
            return true
        } else {
            return false
        }
    }
}

public extension ComponentTransition {
    func animateView(allowUserInteraction: Bool = true, delay: Double = 0.0, _ f: @escaping () -> Void, completion: ((Bool) -> Void)? = nil) {
        switch self.animation {
        case .none:
            f()
            completion?(true)
        case let .curve(duration, curve):
            var options = curve.viewAnimationOptions
            if allowUserInteraction {
                options.insert(.allowUserInteraction)
            }
            switch curve {
            case .spring, .bounce, .custom:
                var parameters: CALayerSpringParametersOverrideParameters?
                var dampingValue: CGFloat = 500.0
                if case let .bounce(stiffness, damping) = curve {
                    dampingValue = damping
                    parameters = CALayerSpringParametersOverrideParametersSpring(stiffness: stiffness, damping: damping, duration: duration)
                } else if case let .custom(a, b, c, d) = curve {
                    parameters = CALayerSpringParametersOverrideParametersCustomCurve(cp1: CGPoint(x: CGFloat(a), y: CGFloat(b)), cp2: CGPoint(x: CGFloat(c), y: CGFloat(d)))
                }
                CALayer.push(CALayerSpringParametersOverride(parameters: parameters))
                UIView.animate(withDuration: duration, delay: delay, usingSpringWithDamping: dampingValue, initialSpringVelocity: 0.0, options: options, animations: {
                    f()
                }, completion: completion)
                CALayer.popSpringParametersOverride()
            default:
                UIView.animate(withDuration: duration, delay: delay, options: options, animations: {
                    f()
                }, completion: completion)
            }
        }
    }
}

public struct ComponentTransition {
    public enum Animation {
        public enum Curve {
            case easeInOut
            case easeIn
            case spring
            case linear
            case custom(Float, Float, Float, Float)
            case bounce(stiffness: CGFloat, damping: CGFloat)
            

            
            public static var slide: Curve {
                return .custom(0.33, 0.52, 0.25, 0.99)
            }
        }

        case none
        case curve(duration: Double, curve: Curve)
    }
    
    public var animation: Animation
    private var _userData: [Any] = []

    public func userData<T>(_ type: T.Type) -> T? {
        for item in self._userData.reversed() {
            if let item = item as? T {
                return item
            }
        }
        return nil
    }

    public func withUserData(_ userData: Any) -> ComponentTransition {
        var result = self
        result._userData.append(userData)
        return result
    }
    
    public func withAnimation(_ animation: Animation) -> ComponentTransition {
        var result = self
        result.animation = animation
        return result
    }
    
    public func withAnimationIfAnimated(_ animation: Animation) -> ComponentTransition {
        switch self.animation {
        case .none:
            return self
        default:
            var result = self
            result.animation = animation
            return result
        }
    }
    
    public static var immediate: ComponentTransition = ComponentTransition(animation: .none)
    
    public static func easeInOut(duration: Double) -> ComponentTransition {
        return ComponentTransition(animation: .curve(duration: duration, curve: .easeInOut))
    }
    
    public static func spring(duration: Double) -> ComponentTransition {
        return ComponentTransition(animation: .curve(duration: duration, curve: .spring))
    }

    public init(animation: Animation) {
        self.animation = animation
    }
    
    public func setFrame(view: UIView, frame: CGRect, completion: ((Bool) -> Void)? = nil) {
        if view.frame == frame {
            completion?(true)
            return
        }
        switch self.animation {
        case .none:
            view.frame = frame
            view.layer.removeAnimation(forKey: "position")
            view.layer.removeAnimation(forKey: "bounds")
            view.layer.removeAnimation(forKey: "bounds.size")
            completion?(true)
        case .curve:
            let previousPosition: CGPoint
            let previousBounds: CGRect
            if (view.layer.animation(forKey: "position") != nil || view.layer.animation(forKey: "bounds") != nil || view.layer.animation(forKey: "bounds.size") != nil), let presentation = view.layer.presentation() {
                previousPosition = presentation.position
                previousBounds = presentation.bounds
            } else {
                previousPosition = view.layer.position
                previousBounds = view.layer.bounds
            }
            
            view.frame = frame
            
            let anchorPoint = view.layer.anchorPoint
            let updatedPosition = CGPoint(x: frame.minX + frame.width * anchorPoint.x, y: frame.minY + frame.height * anchorPoint.y)

            self.animatePosition(view: view, from: previousPosition, to: updatedPosition, completion: completion)
            if previousBounds.size != frame.size {
                self.animateBoundsSize(view: view, from: previousBounds.size, to: frame.size)
            }
        }
    }

    public func setFrame(layer: CALayer, frame: CGRect, completion: ((Bool) -> Void)? = nil) {
        if layer.frame == frame {
            completion?(true)
            return
        }
        switch self.animation {
        case .none:
            layer.frame = frame
            //view.bounds = CGRect(origin: view.bounds.origin, size: frame.size)
            //view.layer.position = CGPoint(x: frame.midX, y: frame.midY)
            layer.removeAnimation(forKey: "position")
            layer.removeAnimation(forKey: "bounds")
            completion?(true)
        case .curve:
            let previousFrame: CGRect
            if (layer.animation(forKey: "position") != nil || layer.animation(forKey: "bounds") != nil), let presentation = layer.presentation() {
                previousFrame = presentation.frame
            } else {
                previousFrame = layer.frame
            }
            
            layer.frame = frame
            //view.bounds = CGRect(origin: previousBounds.origin, size: frame.size)
            //view.center = CGPoint(x: frame.midX, y: frame.midY)

            self.animatePosition(layer: layer, from: CGPoint(x: previousFrame.midX, y: previousFrame.midY), to: CGPoint(x: frame.midX, y: frame.midY), completion: completion)
            self.animateBounds(layer: layer, from: CGRect(origin: layer.bounds.origin, size: previousFrame.size), to: CGRect(origin: layer.bounds.origin, size: frame.size))
        }
    }

    public func setBounds(view: UIView, bounds: CGRect, completion: ((Bool) -> Void)? = nil) {
        if view.bounds == bounds {
            completion?(true)
            return
        }
        switch self.animation {
        case .none:
            view.bounds = bounds
            view.layer.removeAnimation(forKey: "bounds")
            view.layer.removeAnimation(forKey: "bounds.origin")
            view.layer.removeAnimation(forKey: "bounds.size")
            completion?(true)
        case .curve:
            let previousBounds: CGRect
            if (view.layer.animation(forKey: "bounds") != nil || view.layer.animation(forKey: "bounds.origin") != nil || view.layer.animation(forKey: "bounds.size") != nil), let presentation = view.layer.presentation() {
                previousBounds = presentation.bounds
            } else {
                previousBounds = view.layer.bounds
            }
            view.bounds = bounds

            self.animateBounds(view: view, from: previousBounds, to: view.bounds, completion: completion)
        }
    }

    public func setBoundsSize(view: UIView, size: CGSize, completion: ((Bool) -> Void)? = nil) {
        if view.bounds.size == size {
            completion?(true)
            return
        }
        switch self.animation {
        case .none:
            view.bounds.size = size
            view.layer.removeAnimation(forKey: "bounds.size")
            completion?(true)
        case .curve:
            let previousBounds: CGRect
            if view.layer.animation(forKey: "bounds.size") != nil, let presentation = view.layer.presentation() {
                previousBounds = presentation.bounds
            } else {
                previousBounds = view.layer.bounds
            }
            view.bounds = CGRect(origin: view.bounds.origin, size: size)

            self.animateBoundsSize(view: view, from: previousBounds.size, to: size, completion: completion)
        }
    }

    public func setPosition(view: UIView, position: CGPoint, delay: Double = 0.0, completion: ((Bool) -> Void)? = nil) {
        if view.center == position {
            completion?(true)
            return
        }
        switch self.animation {
        case .none:
            view.center = position
            view.layer.removeAnimation(forKey: "position")
            completion?(true)
        case .curve:
            let previousPosition: CGPoint
            if view.layer.animation(forKey: "position") != nil, let presentation = view.layer.presentation() {
                previousPosition = presentation.position
            } else {
                previousPosition = view.layer.position
            }
            view.center = position

            self.animatePosition(view: view, from: previousPosition, to: view.center, delay: delay, completion: completion)
        }
    }

    public func setBounds(layer: CALayer, bounds: CGRect, completion: ((Bool) -> Void)? = nil) {
        if layer.bounds == bounds {
            completion?(true)
            return
        }
        switch self.animation {
        case .none:
            layer.bounds = bounds
            layer.removeAnimation(forKey: "bounds")
            completion?(true)
        case .curve:
            let previousBounds: CGRect
            if layer.animation(forKey: "bounds") != nil, let presentation = layer.presentation() {
                previousBounds = presentation.bounds
            } else {
                previousBounds = layer.bounds
            }
            layer.bounds = bounds

            self.animateBounds(layer: layer, from: previousBounds, to: layer.bounds, completion: completion)
        }
    }

    public func setPosition(layer: CALayer, position: CGPoint, completion: ((Bool) -> Void)? = nil) {
        if layer.position == position {
            completion?(true)
            return
        }
        switch self.animation {
        case .none:
            layer.position = position
            layer.removeAnimation(forKey: "position")
            completion?(true)
        case .curve:
            let previousPosition: CGPoint
            if layer.animation(forKey: "position") != nil, let presentation = layer.presentation() {
                previousPosition = presentation.position
            } else {
                previousPosition = layer.position
            }
            layer.position = position

            self.animatePosition(layer: layer, from: previousPosition, to: layer.position, completion: completion)
        }
    }

    public func setAlpha(view: UIView, alpha: CGFloat, delay: Double = 0.0, completion: ((Bool) -> Void)? = nil) {
        if view.alpha == alpha {
            completion?(true)
            return
        }
        switch self.animation {
        case .none:
            view.alpha = alpha
            view.layer.removeAnimation(forKey: "opacity")
            completion?(true)
        case .curve:
            let previousAlpha: Float
            if view.layer.animation(forKey: "opacity") != nil {
                previousAlpha = view.layer.presentation()?.opacity ?? Float(view.alpha)
            } else {
                previousAlpha = Float(view.alpha)
            }
            view.alpha = alpha
            self.animateAlpha(layer: view.layer, from: CGFloat(previousAlpha), to: alpha, delay: delay, completion: completion)
        }
    }

    public func setAlpha(layer: CALayer, alpha: CGFloat, delay: Double = 0.0, completion: ((Bool) -> Void)? = nil) {
        if layer.opacity == Float(alpha) {
            completion?(true)
            return
        }
        switch self.animation {
        case .none:
            layer.opacity = Float(alpha)
            layer.removeAnimation(forKey: "opacity")
            completion?(true)
        case .curve:
            let previousAlpha: Float
            if layer.animation(forKey: "opacity") != nil {
                previousAlpha = layer.presentation()?.opacity ?? layer.opacity
            } else {
                previousAlpha = layer.opacity
            }
            layer.opacity = Float(alpha)
            self.animateAlpha(layer: layer, from: CGFloat(previousAlpha), to: alpha, delay: delay, completion: completion)
        }
    }

    public func setScale(view: UIView, scale: CGFloat, delay: Double = 0.0, completion: ((Bool) -> Void)? = nil) {
        self.setScale(layer: view.layer, scale: scale, delay: delay, completion: completion)
    }

    public func setScaleWithSpring(view: UIView, scale: CGFloat, delay: Double = 0.0, completion: ((Bool) -> Void)? = nil) {
        self.setScaleWithSpring(layer: view.layer, scale: scale, delay: delay, completion: completion)
    }

    public func setScale(layer: CALayer, scale: CGFloat, delay: Double = 0.0, beginWithCurrentState: Bool = false, completion: ((Bool) -> Void)? = nil) {
        let currentTransform: CATransform3D
        if beginWithCurrentState, layer.animation(forKey: "transform") != nil || layer.animation(forKey: "transform.scale") != nil {
            currentTransform = layer.presentation()?.transform ?? layer.transform
        } else {
            currentTransform = layer.transform
        }
        
        let currentScale = sqrt((currentTransform.m11 * currentTransform.m11) + (currentTransform.m12 * currentTransform.m12) + (currentTransform.m13 * currentTransform.m13))
        if currentScale == scale {
            if let animation = layer.animation(forKey: "transform.scale") as? CABasicAnimation, let toValue = animation.toValue as? NSNumber {
                if toValue.doubleValue == scale {
                    completion?(true)
                    return
                }
            } else {
                completion?(true)
                return
            }
        }
        switch self.animation {
        case .none:
            layer.transform = CATransform3DMakeScale(scale, scale, 1.0)
            completion?(true)
        case let .curve(duration, curve):
            let previousScale = currentScale
            layer.transform = CATransform3DMakeScale(scale, scale, 1.0)
            layer.animate(
                from: previousScale as NSNumber,
                to: scale as NSNumber,
                keyPath: "transform.scale",
                duration: duration,
                delay: delay,
                curve: curve,
                removeOnCompletion: true,
                additive: false,
                completion: completion
            )
        }
    }

    public func setScaleWithSpring(layer: CALayer, scale: CGFloat, delay: Double = 0.0, completion: ((Bool) -> Void)? = nil) {
        let t = layer.presentation()?.transform ?? layer.transform
        let currentScale = sqrt((t.m11 * t.m11) + (t.m12 * t.m12) + (t.m13 * t.m13))
        if currentScale == scale {
            if let animation = layer.animation(forKey: "transform.scale") as? CABasicAnimation, let toValue = animation.toValue as? NSNumber {
                if toValue.doubleValue == scale {
                    completion?(true)
                    return
                }
            } else {
                completion?(true)
                return
            }
        }
        switch self.animation {
        case .none:
            layer.transform = CATransform3DMakeScale(scale, scale, 1.0)
            completion?(true)
        case let .curve(duration, _):
            let previousScale = currentScale
            layer.transform = CATransform3DMakeScale(scale, scale, 1.0)
            layer.animateSpring(
                from: previousScale as NSNumber,
                to: scale as NSNumber,
                keyPath: "transform.scale",
                duration: duration,
                delay: delay,
                removeOnCompletion: true,
                additive: false,
                completion: completion
            )
        }
    }

    public func animateScale(view: UIView, from fromValue: CGFloat, to toValue: CGFloat, delay: Double = 0.0, additive: Bool = false, completion: ((Bool) -> Void)? = nil) {
        self.animateScale(layer: view.layer, from: fromValue, to: toValue, delay: delay, additive: additive, completion: completion)
    }

    public func animateScale(layer: CALayer, from fromValue: CGFloat, to toValue: CGFloat, delay: Double = 0.0, additive: Bool = false, completion: ((Bool) -> Void)? = nil) {
        switch self.animation {
        case .none:
            completion?(true)
        case let .curve(duration, curve):
            layer.animate(
                from: fromValue as NSNumber,
                to: toValue as NSNumber,
                keyPath: "transform.scale",
                duration: duration,
                delay: delay,
                curve: curve,
                removeOnCompletion: true,
                additive: additive,
                completion: completion
            )
        }
    }

    public func animateAlpha(view: UIView, from fromValue: CGFloat, to toValue: CGFloat, delay: Double = 0.0, additive: Bool = false, completion: ((Bool) -> Void)? = nil) {
        self.animateAlpha(layer: view.layer, from: fromValue, to: toValue, delay: delay, additive: additive, completion: completion)
    }

    public func animateAlpha(layer: CALayer, from fromValue: CGFloat, to toValue: CGFloat, delay: Double = 0.0, additive: Bool = false, completion: ((Bool) -> Void)? = nil) {
        switch self.animation {
        case .none:
            completion?(true)
        case let .curve(duration, curve):
            layer.animate(
                from: fromValue as NSNumber,
                to: toValue as NSNumber,
                keyPath: "opacity",
                duration: duration,
                delay: delay,
                curve: curve,
                removeOnCompletion: true,
                additive: additive,
                completion: completion
            )
        }
    }

    public func animatePosition(view: UIView, from fromValue: CGPoint, to toValue: CGPoint, delay: Double = 0.0, additive: Bool = false, completion: ((Bool) -> Void)? = nil) {
        self.animatePosition(layer: view.layer, from: fromValue, to: toValue, delay: delay, additive: additive, completion: completion)
    }

    public func animateBounds(view: UIView, from fromValue: CGRect, to toValue: CGRect, additive: Bool = false, completion: ((Bool) -> Void)? = nil) {
        self.animateBounds(layer: view.layer, from: fromValue, to: toValue, additive: additive, completion: completion)
    }

    public func animateBoundsSize(view: UIView, from fromValue: CGSize, to toValue: CGSize, additive: Bool = false, completion: ((Bool) -> Void)? = nil) {
        self.animateBoundsSize(layer: view.layer, from: fromValue, to: toValue, additive: additive, completion: completion)
    }

    public func animatePosition(layer: CALayer, from fromValue: CGPoint, to toValue: CGPoint, delay: Double = 0.0, additive: Bool = false, completion: ((Bool) -> Void)? = nil) {
        switch self.animation {
        case .none:
            completion?(true)
        case let .curve(duration, curve):
            layer.animate(
                from: NSValue(cgPoint: fromValue),
                to: NSValue(cgPoint: toValue),
                keyPath: "position",
                duration: duration,
                delay: delay,
                curve: curve,
                removeOnCompletion: true,
                additive: additive,
                completion: completion
            )
        }
    }

    public func animateBounds(layer: CALayer, from fromValue: CGRect, to toValue: CGRect, additive: Bool = false, completion: ((Bool) -> Void)? = nil) {
        switch self.animation {
        case .none:
            completion?(true)
        case let .curve(duration, curve):
            layer.animate(
                from: NSValue(cgRect: fromValue),
                to: NSValue(cgRect: toValue),
                keyPath: "bounds",
                duration: duration,
                delay: 0.0,
                curve: curve,
                removeOnCompletion: true,
                additive: additive,
                completion: completion
            )
        }
    }

    public func animateBoundsSize(layer: CALayer, from fromValue: CGSize, to toValue: CGSize, additive: Bool = false, completion: ((Bool) -> Void)? = nil) {
        switch self.animation {
        case .none:
            completion?(true)
        case let .curve(duration, curve):
            layer.animate(
                from: NSValue(cgSize: fromValue),
                to: NSValue(cgSize: toValue),
                keyPath: "bounds.size",
                duration: duration,
                delay: 0.0,
                curve: curve,
                removeOnCompletion: true,
                additive: additive,
                completion: completion
            )
        }
    }

    public func setCornerRadius(layer: CALayer, cornerRadius: CGFloat, completion: ((Bool) -> Void)? = nil) {
        if layer.cornerRadius == cornerRadius {
            completion?(true)
            return
        }
        switch self.animation {
        case .none:
            layer.cornerRadius = cornerRadius
            completion?(true)
        case let .curve(duration, curve):
            let fromValue: CGFloat
            if layer.animation(forKey: "cornerRadius") != nil, let presentation = layer.presentation() {
                fromValue = presentation.cornerRadius
            } else {
                fromValue = layer.cornerRadius
            }
            layer.cornerRadius = cornerRadius
            layer.animate(
                from: fromValue as NSNumber,
                to: cornerRadius as NSNumber,
                keyPath: "cornerRadius",
                duration: duration,
                delay: 0.0,
                curve: curve,
                removeOnCompletion: true,
                additive: false,
                completion: completion
            )
        }
    }

    public func setShapeLayerPath(layer: CAShapeLayer, path: CGPath, completion: ((Bool) -> Void)? = nil) {
        switch self.animation {
        case .none:
            layer.path = path
            completion?(true)
        case let .curve(duration, curve):
            if let previousPath = layer.path, previousPath != path {
                layer.animate(
                    from: previousPath,
                    to: path,
                    keyPath: "path",
                    duration: duration,
                    delay: 0.0,
                    curve: curve,
                    removeOnCompletion: true,
                    additive: false,
                    completion: completion
                )
                layer.path = path
            } else {
                layer.path = path
                completion?(true)
            }
        }
    }

    public func animateMeshTransform(layer: CALayer, from fromValue: NSObject, to toValue: NSObject, delay: Double = 0.0, removeOnCompletion: Bool = true, completion: ((Bool) -> Void)? = nil) {
        switch self.animation {
        case .none:
            completion?(true)
        case let .curve(duration, curve):
            layer.animate(
                from: fromValue,
                to: toValue,
                keyPath: "meshTransform",
                duration: duration,
                delay: delay,
                curve: curve,
                removeOnCompletion: removeOnCompletion,
                additive: false,
                completion: completion
            )
        }
    }
}
