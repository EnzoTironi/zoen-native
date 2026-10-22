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

extension ComponentTransition.Animation.Curve {
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
    
}
