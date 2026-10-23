// Adapted from TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// GPL-2.0-or-later; GPL-3.0 selected. See NOTICE.md and UPSTREAM_SOURCES.json.
import Foundation
import UIKit

public final class GlassBackgroundContainerView: UIView {
    private final class ContentView: UIView {
    }
    
    private let legacyView: ContentView?
    private let nativeParamsView: EffectSettingsContainerView?
    private let nativeView: UIVisualEffectView?
    
    public var contentView: UIView {
        if let nativeView = self.nativeView {
            return nativeView.contentView
        } else {
            return self.legacyView!
        }
    }
    
    public init(spacing: CGFloat = 7.0) {
        if #available(iOS 26.0, *), !GlassBackgroundView.useCustomGlassImpl {
            let effect = UIGlassContainerEffect()
            effect.spacing = spacing
            let nativeView = UIVisualEffectView(effect: effect)
            self.nativeView = nativeView
            
            let nativeParamsView = EffectSettingsContainerView(frame: CGRect())
            self.nativeParamsView = nativeParamsView
            nativeParamsView.addSubview(nativeView)
            
            self.legacyView = nil
        } else {
            self.nativeView = nil
            self.nativeParamsView = nil
            self.legacyView = ContentView()
        }
        
        super.init(frame: CGRect())
        
        if let nativeParamsView = self.nativeParamsView {
            self.addSubview(nativeParamsView)
        } else if let legacyView = self.legacyView {
            self.addSubview(legacyView)
        }
    }
    
    required public init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
    
    override public func didAddSubview(_ subview: UIView) {
        super.didAddSubview(subview)
        
        if subview !== self.nativeParamsView && subview !== self.legacyView {
            assertionFailure()
        }
    }
    
    override public func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        if self.alpha.isZero {
            return nil
        }
        if self.isHidden {
            return nil
        }
        if !self.isUserInteractionEnabled {
            return nil
        }
        for view in self.contentView.subviews.reversed() {
            if let result = view.hitTest(self.convert(point, to: view), with: event), result.isUserInteractionEnabled {
                
                #if DEBUG
                func findMatrix(layer: CALayer) -> AnyObject? {
                    for filter in layer.filters ?? [] {
                        if "\(filter)".contains("vibrantColorMatrix") {
                            return filter as AnyObject
                        }
                    }
                    
                    for sublayer in layer.sublayers ?? [] {
                        if let result = findMatrix(layer: sublayer) {
                            return result
                        }
                    }
                    return nil
                }
                
                /*if let filter = findMatrix(layer: self.layer) as? NSObject {
                    var matrix: [Float32] = .init(repeating: 0, count: 20)
                    let matrixValues = filter.value(forKey: "inputColorMatrix") as! NSValue
                    matrixValues.getValue(&matrix, size: 4 * 20)
                    assert(true)
                }*/
                #endif
                
                return result
            }
        }
        
        guard let result = self.contentView.hitTest(point, with: event) else {
            return nil
        }
        
        if result === self.contentView {
            return nil
        }
        
        return result
    }
    
    public func update(size: CGSize, isDark: Bool, transition: ComponentTransition) {
        if let nativeParamsView = self.nativeParamsView, let nativeView = self.nativeView {
            nativeView.overrideUserInterfaceStyle = isDark ? .dark : .light
            
            if isDark {
                nativeParamsView.lumaMin = 0.0
                nativeParamsView.lumaMax = 0.15
            } else {
                nativeParamsView.lumaMin = 0.8
                nativeParamsView.lumaMax = 0.801
            }
            if let nativeParamsView = self.nativeParamsView {
                transition.setFrame(view: nativeParamsView, frame: CGRect(origin: CGPoint(x: 0.0, y: 0.0), size: size))
            }
            
            transition.animateView {
                nativeView.frame = CGRect(origin: CGPoint(), size: size)
            }
        } else if let legacyView = self.legacyView {
            transition.setFrame(view: legacyView, frame: CGRect(origin: CGPoint(), size: size))
        }
    }
    
    override public func willMove(toWindow newWindow: UIWindow?) {
        super.willMove(toWindow: newWindow)
    }
}
