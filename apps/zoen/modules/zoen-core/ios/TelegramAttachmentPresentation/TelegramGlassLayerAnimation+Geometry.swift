// Adapted from TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// GPL-2.0-or-later; GPL-3.0 selected. See NOTICE.md and UPSTREAM_SOURCES.json.
import UIKit

public extension CALayer {
    func animateAlpha(from: CGFloat, to: CGFloat, duration: Double, delay: Double = 0.0, timingFunction: String = CAMediaTimingFunctionName.easeInEaseOut.rawValue, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, completion: ((Bool) -> ())? = nil) {
        self.animate(from: NSNumber(value: Float(from)), to: NSNumber(value: Float(to)), keyPath: "opacity", timingFunction: timingFunction, duration: duration, delay: delay, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, completion: completion)
    }
    
    func animateScale(from: CGFloat, to: CGFloat, duration: Double, delay: Double = 0.0, timingFunction: String = CAMediaTimingFunctionName.easeInEaseOut.rawValue, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, additive: Bool = false, completion: ((Bool) -> Void)? = nil) {
        self.animate(from: NSNumber(value: Float(from)), to: NSNumber(value: Float(to)), keyPath: "transform.scale", timingFunction: timingFunction, duration: duration, delay: delay, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, additive: additive, completion: completion)
    }
    
    func animateSublayerScale(from: CGFloat, to: CGFloat, duration: Double, delay: Double = 0.0, timingFunction: String = CAMediaTimingFunctionName.easeInEaseOut.rawValue, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, additive: Bool = false, completion: ((Bool) -> Void)? = nil) {
        self.animate(from: NSNumber(value: Float(from)), to: NSNumber(value: Float(to)), keyPath: "sublayerTransform.scale", timingFunction: timingFunction, duration: duration, delay: delay, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, additive: additive, completion: completion)
    }

    func animateScaleX(from: CGFloat, to: CGFloat, duration: Double, delay: Double = 0.0, timingFunction: String = CAMediaTimingFunctionName.easeInEaseOut.rawValue, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, completion: ((Bool) -> Void)? = nil) {
        self.animate(from: NSNumber(value: Float(from)), to: NSNumber(value: Float(to)), keyPath: "transform.scale.x", timingFunction: timingFunction, duration: duration, delay: delay, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, completion: completion)
    }
    
    func animateScaleY(from: CGFloat, to: CGFloat, duration: Double, delay: Double = 0.0, timingFunction: String = CAMediaTimingFunctionName.easeInEaseOut.rawValue, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, completion: ((Bool) -> Void)? = nil) {
        self.animate(from: NSNumber(value: Float(from)), to: NSNumber(value: Float(to)), keyPath: "transform.scale.y", timingFunction: timingFunction, duration: duration, delay: delay, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, completion: completion)
    }
    
    func animateRotation(from: CGFloat, to: CGFloat, duration: Double, delay: Double = 0.0, timingFunction: String = CAMediaTimingFunctionName.easeInEaseOut.rawValue, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, completion: ((Bool) -> Void)? = nil) {
        self.animate(from: NSNumber(value: Float(from)), to: NSNumber(value: Float(to)), keyPath: "transform.rotation.z", timingFunction: timingFunction, duration: duration, delay: delay, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, completion: completion)
    }
    
    func animatePosition(from: CGPoint, to: CGPoint, duration: Double, delay: Double = 0.0, timingFunction: String = CAMediaTimingFunctionName.easeInEaseOut.rawValue, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, additive: Bool = false, force: Bool = false, completion: ((Bool) -> Void)? = nil) {
        if from == to && !force {
            if let completion = completion {
                completion(true)
            }
            return
        }
        self.animate(from: NSValue(cgPoint: from), to: NSValue(cgPoint: to), keyPath: "position", timingFunction: timingFunction, duration: duration, delay: delay, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, additive: additive, completion: completion)
    }
    
    func animateAnchorPoint(from: CGPoint, to: CGPoint, duration: Double, delay: Double = 0.0, timingFunction: String = CAMediaTimingFunctionName.easeInEaseOut.rawValue, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, additive: Bool = false, force: Bool = false, completion: ((Bool) -> Void)? = nil) {
        if from == to && !force {
            if let completion = completion {
                completion(true)
            }
            return
        }
        self.animate(from: NSValue(cgPoint: from), to: NSValue(cgPoint: to), keyPath: "anchorPoint", timingFunction: timingFunction, duration: duration, delay: delay, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, additive: additive, completion: completion)
    }
    
    func animateBounds(from: CGRect, to: CGRect, duration: Double, delay: Double = 0.0, timingFunction: String, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, additive: Bool = false, force: Bool = false, completion: ((Bool) -> Void)? = nil) {
        if from == to && !force {
            if let completion = completion {
                completion(true)
            }
            return
        }
        self.animate(from: NSValue(cgRect: from), to: NSValue(cgRect: to), keyPath: "bounds", timingFunction: timingFunction, duration: duration, delay: delay, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, additive: additive, completion: completion)
    }

    func animateWidth(from: CGFloat, to: CGFloat, duration: Double, delay: Double = 0.0, timingFunction: String, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, additive: Bool = false, force: Bool = false, completion: ((Bool) -> Void)? = nil) {
        if from == to && !force {
            if let completion = completion {
                completion(true)
            }
            return
        }
        self.animate(from: from as NSNumber, to: to as NSNumber, keyPath: "bounds.size.width", timingFunction: timingFunction, duration: duration, delay: delay, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, additive: additive, completion: completion)
    }

    func animateHeight(from: CGFloat, to: CGFloat, duration: Double, delay: Double = 0.0, timingFunction: String, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, additive: Bool = false, force: Bool = false, completion: ((Bool) -> Void)? = nil) {
        if from == to && !force {
            if let completion = completion {
                completion(true)
            }
            return
        }
        self.animate(from: from as NSNumber, to: to as NSNumber, keyPath: "bounds.size.height", timingFunction: timingFunction, duration: duration, delay: delay, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, additive: additive, completion: completion)
    }
    
    func animateBoundsOriginXAdditive(from: CGFloat, to: CGFloat, duration: Double, delay: Double = 0.0, timingFunction: String = CAMediaTimingFunctionName.easeInEaseOut.rawValue, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, completion: ((Bool) -> Void)? = nil) {
        self.animate(from: from as NSNumber, to: to as NSNumber, keyPath: "bounds.origin.x", timingFunction: timingFunction, duration: duration, delay: delay, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, additive: true, completion: completion)
    }
    
    func animateBoundsOriginYAdditive(from: CGFloat, to: CGFloat, duration: Double, timingFunction: String = CAMediaTimingFunctionName.easeInEaseOut.rawValue, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, completion: ((Bool) -> Void)? = nil) {
        self.animate(from: from as NSNumber, to: to as NSNumber, keyPath: "bounds.origin.y", timingFunction: timingFunction, duration: duration, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, additive: true, completion: completion)
    }
    
    func animateBoundsOriginXAdditive(from: CGFloat, to: CGFloat, duration: Double, mediaTimingFunction: CAMediaTimingFunction) {
        self.animate(from: from as NSNumber, to: to as NSNumber, keyPath: "bounds.origin.x", timingFunction: CAMediaTimingFunctionName.easeInEaseOut.rawValue, duration: duration, mediaTimingFunction: mediaTimingFunction, additive: true)
    }
    
    func animateBoundsOriginYAdditive(from: CGFloat, to: CGFloat, duration: Double, mediaTimingFunction: CAMediaTimingFunction) {
        self.animate(from: from as NSNumber, to: to as NSNumber, keyPath: "bounds.origin.y", timingFunction: CAMediaTimingFunctionName.easeInEaseOut.rawValue, duration: duration, mediaTimingFunction: mediaTimingFunction, additive: true)
    }
    
    func animateBoundsOriginAdditive(from: CGPoint, to: CGPoint, duration: Double, mediaTimingFunction: CAMediaTimingFunction) {
        self.animate(from: NSValue(cgPoint: from), to: NSValue(cgPoint: to), keyPath: "bounds.origin", timingFunction: CAMediaTimingFunctionName.easeInEaseOut.rawValue, duration: duration, mediaTimingFunction: mediaTimingFunction, additive: true)
    }
    
    func animateBoundsOriginAdditive(from: CGPoint, to: CGPoint, duration: Double, timingFunction: String = CAMediaTimingFunctionName.easeInEaseOut.rawValue, mediaTimingFunction: CAMediaTimingFunction? = nil) {
        self.animate(from: NSValue(cgPoint: from), to: NSValue(cgPoint: to), keyPath: "bounds.origin", timingFunction: timingFunction, duration: duration, mediaTimingFunction: mediaTimingFunction, additive: true)
    }
    
    func animateShapeLineWidth(from: CGFloat, to: CGFloat, duration: Double, delay: Double = 0.0, timingFunction: String = CAMediaTimingFunctionName.easeInEaseOut.rawValue, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, additive: Bool = false, completion: ((Bool) -> Void)? = nil) {
        self.animate(from: NSNumber(value: Float(from)), to: NSNumber(value: Float(to)), keyPath: "lineWidth", timingFunction: timingFunction, duration: duration, delay: delay, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, additive: additive, completion: completion)
    }
    
    func animatePositionKeyframes(values: [CGPoint], duration: Double, removeOnCompletion: Bool = true, completion: ((Bool) -> Void)? = nil) {
        self.animateKeyframes(values: values.map { NSValue(cgPoint: $0) }, duration: duration, keyPath: "position")
    }
    
    func animateFrame(from: CGRect, to: CGRect, duration: Double, delay: Double = 0.0, timingFunction: String = CAMediaTimingFunctionName.easeInEaseOut.rawValue, mediaTimingFunction: CAMediaTimingFunction? = nil, removeOnCompletion: Bool = true, additive: Bool = false, force: Bool = false, completion: ((Bool) -> Void)? = nil) {
        if from == to && !force {
            if let completion = completion {
                completion(true)
            }
            return
        }
        var interrupted = false
        var completedPosition = false
        var completedBounds = false
        let partialCompletion: () -> Void = {
            if interrupted || (completedPosition && completedBounds) {
                if let completion = completion {
                    completion(!interrupted)
                }
            }
        }
        
        var fromPosition = CGPoint(x: from.midX, y: from.midY)
        var toPosition = CGPoint(x: to.midX, y: to.midY)
        
        var fromBounds = CGRect(origin: self.bounds.origin, size: from.size)
        var toBounds = CGRect(origin: self.bounds.origin, size: to.size)
        
        if additive {
            fromPosition.x = -(toPosition.x - fromPosition.x)
            fromPosition.y = -(toPosition.y - fromPosition.y)
            toPosition = CGPoint()
            
            fromBounds.size.width = -(toBounds.width - fromBounds.width)
            fromBounds.size.height = -(toBounds.height - fromBounds.height)
            toBounds = CGRect()
        }
        
        self.animatePosition(from: fromPosition, to: toPosition, duration: duration, delay: delay, timingFunction: timingFunction, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, additive: additive, force: force, completion: { value in
            if !value {
                interrupted = true
            }
            completedPosition = true
            partialCompletion()
        })
        self.animateBounds(from: fromBounds, to: toBounds, duration: duration, delay: delay, timingFunction: timingFunction, mediaTimingFunction: mediaTimingFunction, removeOnCompletion: removeOnCompletion, additive: additive, force: force, completion: { value in
            if !value {
                interrupted = true
            }
            completedBounds = true
            partialCompletion()
        })
    }
    
    func cancelAnimationsRecursive(key: String) {
        self.removeAnimation(forKey: key)
        if let sublayers = self.sublayers {
            for layer in sublayers {
                layer.cancelAnimationsRecursive(key: key)
            }
        }
    }
}
