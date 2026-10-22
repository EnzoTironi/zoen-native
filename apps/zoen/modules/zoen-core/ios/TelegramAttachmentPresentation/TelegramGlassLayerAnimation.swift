// Adapted from TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// GPL-2.0-or-later; GPL-3.0 selected. See NOTICE.md and UPSTREAM_SOURCES.json.
import UIKit

@objc private class CALayerAnimationDelegate: NSObject, CAAnimationDelegate {
    private let keyPath: String?
    var completion: ((Bool) -> Void)?
    
    init(animation: CAAnimation, completion: ((Bool) -> Void)?) {
        if let animation = animation as? CABasicAnimation {
            self.keyPath = animation.keyPath
        } else {
            self.keyPath = nil
        }
        self.completion = completion
        
        super.init()
    }
    
    @objc func animationDidStop(_ anim: CAAnimation, finished flag: Bool) {
        if let anim = anim as? CABasicAnimation {
            if anim.keyPath != self.keyPath {
                return
            }
        }
        if let completion = self.completion {
            completion(flag)
            self.completion = nil
        }
    }
}

private let completionKey = "CAAnimationUtils_completion"

public let kCAMediaTimingFunctionSpring = "CAAnimationUtilsSpringCurve"
public let kCAMediaTimingFunctionCustomSpringPrefix = "CAAnimationUtilsSpringCustomCurve"

public extension CAAnimation {
    var completion: ((Bool) -> Void)? {
        get {
            if let delegate = self.delegate as? CALayerAnimationDelegate {
                return delegate.completion
            } else {
                return nil
            }
        } set(value) {
            if let delegate = self.delegate as? CALayerAnimationDelegate {
                delegate.completion = value
            } else {
                self.delegate = CALayerAnimationDelegate(animation: self, completion: value)
            }
        }
    }
}

private func adjustFrameRate(animation: CAAnimation) {
    if #available(iOS 15.0, *) {
        let maxFps = Float(UIScreen.main.maximumFramesPerSecond)
        if maxFps > 61.0 {
            var preferredFps: Float = maxFps
            if let animation = animation as? CABasicAnimation {
                if animation.keyPath == "opacity" {
                    preferredFps = 60.0
                    return
                }
            }
            animation.preferredFrameRateRange = CAFrameRateRange(minimum: 30.0, maximum: preferredFps, preferred: maxFps)
        }
    }
}

public extension CALayer {
    func makeAnimation(from: Any?, to: Any, keyPath: String, timingFunction: String, duration: Double, delay: Double = 0.0, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, additive: Bool = false, completion: ((Bool) -> Void)? = nil) -> CAAnimation {
        if timingFunction.hasPrefix(kCAMediaTimingFunctionCustomSpringPrefix) {
            let components = timingFunction.components(separatedBy: "_")
            let mass: Float
            let stiffness: Float
            let damping: Float
            let initialVelocity: Float
            if components.count >= 5 {
                mass = Float(components[1]) ?? 5.0
                stiffness = Float(components[2]) ?? 900.0
                damping = Float(components[3]) ?? 100.0
                initialVelocity = Float(components[4]) ?? 0.0
            } else {
                mass = 5.0
                stiffness = 900.0
                damping = components.count > 1 ? (Float(components[1]) ?? 100.0) : 100.0
                initialVelocity = components.count > 2 ? (Float(components[2]) ?? 0.0) : 0.0
            }
            
            let animation = CASpringAnimation(keyPath: keyPath)
            animation.fromValue = from
            animation.toValue = to
            animation.isRemovedOnCompletion = removeOnCompletion
            animation.fillMode = .forwards
            if let completion = completion {
                animation.delegate = CALayerAnimationDelegate(animation: animation, completion: completion)
            }
            animation.mass = CGFloat(mass)
            animation.stiffness = CGFloat(stiffness)
            animation.damping = CGFloat(damping)
            animation.initialVelocity = CGFloat(initialVelocity)
            animation.duration = animation.settlingDuration
            animation.timingFunction = CAMediaTimingFunction.init(name: .linear)
            let k = Float(UIView.animationDurationFactor)
            var speed: Float = 1.0
            if k != 0 && k != 1 {
                speed = Float(1.0) / k
            }
            animation.speed = speed * Float(animation.duration / duration)
            animation.isAdditive = additive
            if !delay.isZero {
                animation.beginTime = self.convertTime(CACurrentMediaTime(), from: nil) + delay * UIView.animationDurationFactor
                animation.fillMode = .both
            }
            adjustFrameRate(animation: animation)
            
            return animation
        } else if timingFunction == kCAMediaTimingFunctionSpring {
            if #available(iOS 26.0, *), abs(duration - 0.3832) <= 0.0001 {
                let animation = make26SpringAnimationImpl(keyPath, duration)
                animation.fromValue = from
                animation.toValue = to
                animation.isRemovedOnCompletion = removeOnCompletion
                animation.fillMode = .forwards
                if let completion {
                    animation.delegate = CALayerAnimationDelegate(animation: animation, completion: completion)
                }
                
                let k = Float(UIView.animationDurationFactor)
                var speed: Float = 1.0
                if k != 0 && k != 1 {
                    speed = Float(1.0) / k
                }
                
                animation.speed = speed * Float(animation.duration / duration)
                animation.isAdditive = additive
                
                if !delay.isZero {
                    animation.beginTime = self.convertTime(CACurrentMediaTime(), from: nil) + delay * UIView.animationDurationFactor
                    animation.fillMode = .both
                }
                
                adjustFrameRate(animation: animation)
                
                return animation
            } else if duration == 0.5 {
                let animation = makeSpringAnimation(keyPath, duration: duration)
                animation.fromValue = from
                animation.toValue = to
                animation.isRemovedOnCompletion = removeOnCompletion
                animation.fillMode = .forwards
                if let completion = completion {
                    animation.delegate = CALayerAnimationDelegate(animation: animation, completion: completion)
                }
                
                let k = Float(UIView.animationDurationFactor)
                var speed: Float = 1.0
                if k != 0 && k != 1 {
                    speed = Float(1.0) / k
                }
                
                animation.speed = speed * Float(animation.duration / duration)
                animation.isAdditive = additive
                
                if !delay.isZero {
                    animation.beginTime = self.convertTime(CACurrentMediaTime(), from: nil) + delay * UIView.animationDurationFactor
                    animation.fillMode = .both
                }
                
                adjustFrameRate(animation: animation)
                
                return animation
            } else {
                let k = Float(UIView.animationDurationFactor)
                var speed: Float = 1.0
                if k != 0 && k != 1 {
                    speed = Float(1.0) / k
                }
                
                let animation = CABasicAnimation(keyPath: keyPath)
                animation.fromValue = from
                animation.toValue = to
                animation.duration = duration
                
                animation.timingFunction = CAMediaTimingFunction(controlPoints: 0.380, 0.700, 0.125, 1.000)
                
                animation.isRemovedOnCompletion = removeOnCompletion
                animation.fillMode = .forwards
                animation.speed = speed
                animation.isAdditive = additive
                if let completion = completion {
                    animation.delegate = CALayerAnimationDelegate(animation: animation, completion: completion)
                }
                
                if !delay.isZero {
                    animation.beginTime = self.convertTime(CACurrentMediaTime(), from: nil) + delay * UIView.animationDurationFactor
                    animation.fillMode = .both
                }
                
                adjustFrameRate(animation: animation)
                
                return animation
            }
        } else {
            let k = Float(UIView.animationDurationFactor)
            var speed: Float = 1.0
            if k != 0 && k != 1 {
                speed = Float(1.0) / k
            }
            
            let animation = CABasicAnimation(keyPath: keyPath)
            animation.fromValue = from
            animation.toValue = to
            animation.duration = duration
            if let mediaTimingFunction = mediaTimingFunction {
                animation.timingFunction = mediaTimingFunction
            } else {
                switch timingFunction {
                case CAMediaTimingFunctionName.linear.rawValue, CAMediaTimingFunctionName.easeIn.rawValue, CAMediaTimingFunctionName.easeOut.rawValue, CAMediaTimingFunctionName.easeInEaseOut.rawValue, CAMediaTimingFunctionName.default.rawValue:
                    animation.timingFunction = CAMediaTimingFunction(name: CAMediaTimingFunctionName(rawValue: timingFunction))
                default:
                    animation.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
                }
                
            }
            animation.isRemovedOnCompletion = removeOnCompletion
            animation.fillMode = .forwards
            animation.speed = speed
            animation.isAdditive = additive
            if let completion = completion {
                animation.delegate = CALayerAnimationDelegate(animation: animation, completion: completion)
            }
            
            if !delay.isZero {
                animation.beginTime = self.convertTime(CACurrentMediaTime(), from: nil) + delay * UIView.animationDurationFactor
                animation.fillMode = .both
            }
            
            adjustFrameRate(animation: animation)
            
            return animation
        }
    }
    
    func animate(from: Any?, to: Any, keyPath: String, timingFunction: String, duration: Double, delay: Double = 0.0, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, additive: Bool = false, completion: ((Bool) -> Void)? = nil, key: String? = nil) {
        let animation = self.makeAnimation(from: from, to: to, keyPath: keyPath, timingFunction: timingFunction, duration: duration, delay: delay, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, additive: additive, completion: completion)
        self.add(animation, forKey: key ?? (additive ? nil : keyPath))
    }
    
    func animateGroup(_ animations: [CAAnimation], key: String, completion: ((Bool) -> Void)? = nil) {
        let animationGroup = CAAnimationGroup()
        var timeOffset = 0.0
        for animation in animations {
            animation.beginTime = self.convertTime(animation.beginTime, from: nil) + timeOffset
            timeOffset += animation.duration / Double(animation.speed)
        }
        animationGroup.animations = animations
        animationGroup.duration = timeOffset
        if let completion = completion {
            animationGroup.delegate = CALayerAnimationDelegate(animation: animationGroup, completion: completion)
        }
        
        adjustFrameRate(animation: animationGroup)
        
        self.add(animationGroup, forKey: key)
    }
    
    func animateKeyframes(values: [AnyObject], keyTimes: [NSNumber]? = nil, duration: Double, keyPath: String, timingFunction: String = CAMediaTimingFunctionName.linear.rawValue, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, additive: Bool = false, completion: ((Bool) -> Void)? = nil) {
        let k = Float(UIView.animationDurationFactor)
        var speed: Float = 1.0
        if k != 0 && k != 1 {
            speed = Float(1.0) / k
        }
        
        let animation = CAKeyframeAnimation(keyPath: keyPath)
        animation.values = values
        var effectiveKeyTimes: [NSNumber] = []
        if let keyTimes {
            effectiveKeyTimes = keyTimes
        } else {
            for i in 0 ..< values.count {
                if i == 0 {
                    effectiveKeyTimes.append(0.0)
                } else if i == values.count - 1 {
                    effectiveKeyTimes.append(1.0)
                } else {
                    effectiveKeyTimes.append((Double(i) / Double(values.count - 1)) as NSNumber)
                }
            }
        }
        animation.keyTimes = effectiveKeyTimes
        animation.speed = speed
        animation.duration = duration
        animation.isAdditive = additive
        animation.calculationMode = .linear
        if let mediaTimingFunction = mediaTimingFunction {
            animation.timingFunction = mediaTimingFunction
        } else {
            animation.timingFunction = CAMediaTimingFunction(name: CAMediaTimingFunctionName(rawValue: timingFunction))
        }
        animation.isRemovedOnCompletion = removeOnCompletion
        if let completion = completion {
            animation.delegate = CALayerAnimationDelegate(animation: animation, completion: completion)
        }
        
        adjustFrameRate(animation: animation)
        
        self.add(animation, forKey: keyPath)
    }
    
    func springAnimation(from: AnyObject, to: AnyObject, keyPath: String, duration: Double, delay: Double = 0.0, initialVelocity: CGFloat = 0.0, damping: CGFloat = 88.0, removeOnCompletion: Bool = true, additive: Bool = false) -> CABasicAnimation {
        let animation = makeSpringBounceAnimation(keyPath, initialVelocity, damping)
        animation.fromValue = from
        animation.toValue = to
        animation.isRemovedOnCompletion = removeOnCompletion
        animation.fillMode = .forwards
        
        let k = Float(UIView.animationDurationFactor)
        var speed: Float = 1.0
        if k != 0 && k != 1 {
            speed = Float(1.0) / k
        }
        
        if !delay.isZero {
            animation.beginTime = self.convertTime(CACurrentMediaTime(), from: nil) + delay * UIView.animationDurationFactor
            animation.fillMode = .both
        }
        
        animation.speed = speed * Float(animation.duration / duration)
        animation.isAdditive = additive
        
        adjustFrameRate(animation: animation)
        
        return animation
    }

    func animateSpring(from: Any, to: Any, keyPath: String, duration: Double, delay: Double = 0.0, initialVelocity: CGFloat = 0.0, stiffness: CGFloat = 900.0, damping: CGFloat = 88.0, removeOnCompletion: Bool = true, additive: Bool = false, completion: ((Bool) -> Void)? = nil, key: String? = nil) {
        let animation = makeSpringBounceAnimation(keyPath, initialVelocity, damping)
        animation.stiffness = stiffness
        animation.fromValue = from
        animation.toValue = to
        animation.isRemovedOnCompletion = removeOnCompletion
        animation.fillMode = .forwards
        if let completion = completion {
            animation.delegate = CALayerAnimationDelegate(animation: animation, completion: completion)
        }
        
        let k = Float(UIView.animationDurationFactor)
        var speed: Float = 1.0
        if k != 0 && k != 1 {
            speed = Float(1.0) / k
        }
        
        if !delay.isZero {
            animation.beginTime = self.convertTime(CACurrentMediaTime(), from: nil) + delay * UIView.animationDurationFactor
            animation.fillMode = .both
        }
        
        animation.speed = speed * Float(animation.duration / duration)
        animation.isAdditive = additive
        
        adjustFrameRate(animation: animation)
        
        self.add(animation, forKey: additive ? key : (key ?? keyPath))
    }
    
    func animateAdditive(from: NSValue, to: NSValue, keyPath: String, key: String, timingFunction: String, mediaTimingFunction: CAMediaTimingFunction? = nil, duration: Double, removeOnCompletion: Bool = true, completion: ((Bool) -> Void)? = nil) {
        let k = Float(UIView.animationDurationFactor)
        var speed: Float = 1.0
        if k != 0 && k != 1 {
            speed = Float(1.0) / k
        }
        
        let animation = CABasicAnimation(keyPath: keyPath)
        animation.fromValue = from
        animation.toValue = to
        animation.duration = duration
        if let mediaTimingFunction = mediaTimingFunction {
            animation.timingFunction = mediaTimingFunction
        } else {
            animation.timingFunction = CAMediaTimingFunction(name: CAMediaTimingFunctionName(rawValue: timingFunction))
        }
        animation.isRemovedOnCompletion = removeOnCompletion
        animation.fillMode = .forwards
        animation.speed = speed
        animation.isAdditive = true
        if let completion = completion {
            animation.delegate = CALayerAnimationDelegate(animation: animation, completion: completion)
        }
        
        adjustFrameRate(animation: animation)
        
        self.add(animation, forKey: key)
    }
    
}
