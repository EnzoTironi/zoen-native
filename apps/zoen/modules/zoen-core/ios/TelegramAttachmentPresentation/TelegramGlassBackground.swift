// Adapted from TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// GPL-2.0-or-later; GPL-3.0 selected. See NOTICE.md and UPSTREAM_SOURCES.json.
import Foundation
import UIKit

final class ContentContainer: UIView {
    private let maskContentView: UIView
    
    init(maskContentView: UIView) {
        self.maskContentView = maskContentView
        
        super.init(frame: CGRect())
    }
    
    required public init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
    
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        guard let result = super.hitTest(point, with: event) else {
            return nil
        }
        if result === self {
            if let gestureRecognizers = self.gestureRecognizers, !gestureRecognizers.isEmpty {
                return result
            }
            return nil
        }
        return result
    }
    
    override func didAddSubview(_ subview: UIView) {
        super.didAddSubview(subview)
        
        if let subview = subview as? GlassBackgroundView.ContentView {
            self.maskContentView.addSubview(subview.tintMask)
        }
    }
    
    override func willRemoveSubview(_ subview: UIView) {
        super.willRemoveSubview(subview)
        
        if let subview = subview as? GlassBackgroundView.ContentView {
            subview.tintMask.removeFromSuperview()
        }
    }
}

public class GlassBackgroundView: UIView {

    public struct Params: Equatable {
        public let shape: Shape
        public let isDark: Bool
        public let tintColor: TintColor
        public let isInteractive: Bool
        public let isVisible: Bool
        
        init(shape: Shape, isDark: Bool, tintColor: TintColor, isInteractive: Bool, isVisible: Bool) {
            self.shape = shape
            self.isDark = isDark
            self.tintColor = tintColor
            self.isInteractive = isInteractive
            self.isVisible = isVisible
        }
    }
    
    let legacyView: LegacyGlassView?
    let legacyHighlightContainerView: UIView?
    let legacyHighlightClippingContext: ClippingShapeContext?
    var glassHighlightRecognizer: GlassHighlightGestureRecognizer?
    
    let nativeView: UIVisualEffectView?
    let nativeViewClippingContext: ClippingShapeContext?
    let nativeParamsView: EffectSettingsContainerView?
    
    let foregroundView: UIImageView?
    let shadowView: UIImageView?
    
    let maskContainerView: UIView
    public let maskContentView: UIView
    let contentContainer: ContentContainer
    
    var innerBackgroundView: UIView?
    
    /// UIKit's liquid morph must target the effect view itself so the backdrop
    /// participates in the shape transition along with its content.
    public var transitionView: UIView {
        return self.nativeView ?? self
    }

    public var contentView: UIView {
        if let nativeView = self.nativeView {
            return nativeView.contentView
        } else {
            return self.contentContainer
        }
    }
    
    public internal(set) var params: Params?
        
    public static var useCustomGlassImpl: Bool = false
    
    public override init(frame: CGRect) {
        if #available(iOS 26.0, *), !GlassBackgroundView.useCustomGlassImpl {
            self.legacyView = nil
            self.legacyHighlightContainerView = nil
            self.legacyHighlightClippingContext = nil
            
            let glassEffect = UIGlassEffect(style: .regular)
            glassEffect.isInteractive = false
            let nativeView = UIVisualEffectView(effect: glassEffect)
            self.nativeViewClippingContext = ClippingShapeContext(view: nativeView)
            self.nativeView = nativeView
            
            let nativeParamsView = EffectSettingsContainerView(frame: CGRect())
            self.nativeParamsView = nativeParamsView
            
            nativeParamsView.addSubview(nativeView)
            
            self.foregroundView = nil
            self.shadowView = nil
        } else {
            self.legacyView = LegacyGlassView(frame: CGRect())
            let legacyHighlightContainerView = UIView()
            legacyHighlightContainerView.isUserInteractionEnabled = false
            legacyHighlightContainerView.clipsToBounds = true
            self.legacyHighlightContainerView = legacyHighlightContainerView
            self.legacyHighlightClippingContext = ClippingShapeContext(view: legacyHighlightContainerView)
            self.nativeView = nil
            self.nativeViewClippingContext = nil
            self.nativeParamsView = nil
            self.foregroundView = UIImageView()
            
            self.shadowView = UIImageView()
        }
        
        self.maskContainerView = UIView()
        self.maskContainerView.backgroundColor = .white
        if let filter = CALayer.luminanceToAlpha() {
            self.maskContainerView.layer.filters = [filter]
        }
        
        self.maskContentView = UIView()
        self.maskContainerView.addSubview(self.maskContentView)
        
        self.contentContainer = ContentContainer(maskContentView: self.maskContentView)
        
        super.init(frame: frame)
        
        if let shadowView = self.shadowView {
            self.addSubview(shadowView)
        }
        if let nativeParamsView = self.nativeParamsView {
            self.addSubview(nativeParamsView)
        }
        if let legacyView = self.legacyView {
            self.addSubview(legacyView)
            let glassHighlightRecognizer = GlassHighlightGestureRecognizer(target: self, action: #selector(self.onHighlightGesture(_:)))
            glassHighlightRecognizer.highlightContainerView = self.legacyHighlightContainerView
            self.glassHighlightRecognizer = glassHighlightRecognizer
            self.addGestureRecognizer(glassHighlightRecognizer)
            glassHighlightRecognizer.isEnabled = false
        }
        if let foregroundView = self.foregroundView {
            self.addSubview(foregroundView)
            foregroundView.mask = self.maskContainerView
        }
        self.addSubview(self.contentContainer)
        if let legacyHighlightContainerView = self.legacyHighlightContainerView {
            self.addSubview(legacyHighlightContainerView)
        }
    }
    
    required public init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
    
    @objc private func onHighlightGesture(_ recognizer: GlassHighlightGestureRecognizer) {
    }
    
    override public func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        if !self.isUserInteractionEnabled {
            return nil
        }
        if self.isHidden {
            return nil
        }
        if self.alpha == 0.0 {
            return nil
        }
        if let nativeView = self.nativeView {
            if let result = nativeView.hitTest(self.convert(point, to: nativeView), with: event) {
                return result
            }
        } else {
            if let result = self.contentContainer.hitTest(self.convert(point, to: self.contentContainer), with: event) {
                return result
            }
        }
        return nil
    }
    

    override public func willMove(toWindow newWindow: UIWindow?) {
        super.willMove(toWindow: newWindow)
    }
}
