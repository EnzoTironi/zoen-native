// Adapted from TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// GPL-2.0-or-later; GPL-3.0 selected. See NOTICE.md and UPSTREAM_SOURCES.json.
import Foundation
import UIKit

final class RestingBackgroundView: UIVisualEffectView {
    var isDark: Bool?

    static func colorMatrix(isDark: Bool) -> [Float32] {
        if isDark {
            return [1.082, -0.113, -0.011, 0.0, 0.135, -0.034, 1.003, -0.011, 0.0, 0.135, -0.034, -0.113, 1.105, 0.0, 0.135, 0.0, 0.0, 0.0, 1.0, 0.0]
        } else {
            return [1.185, -0.05, -0.005, 0.0, -0.2, -0.015, 1.15, -0.005, 0.0, -0.2, -0.015, -0.05, 1.195, 0.0, -0.2, 0.0, 0.0, 0.0, 1.0, 0.0]
        }
    }

    init() {
        let effect = UIBlurEffect(style: .light)
        super.init(effect: effect)
        
        for subview in self.subviews {
            if subview.description.contains("VisualEffectSubview") {
                subview.isHidden = true
            }
        }
        
        self.clipsToBounds = true
    }
    
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func update(isDark: Bool) {
        if self.isDark == isDark {
            return
        }
        self.isDark = isDark
        
        if let sublayer = self.layer.sublayers?[0], let _ = sublayer.filters {
            sublayer.backgroundColor = nil
            sublayer.isOpaque = false
            
            if let classValue = NSClassFromString("CAFilter") as AnyObject as? NSObjectProtocol {
                let makeSelector = NSSelectorFromString("filterWithName:")
                let filter = classValue.perform(makeSelector, with: "colorMatrix").takeUnretainedValue() as? NSObject
                
                if let filter {
                    var matrix: [Float32] = RestingBackgroundView.colorMatrix(isDark: isDark)
                    filter.setValue(NSValue(bytes: &matrix, objCType: "{CAColorMatrix=ffffffffffffffffffff}"), forKey: "inputColorMatrix")
                    sublayer.filters = [filter]
                    sublayer.setValue(1.0, forKey: "scale")
                }
            }
        }
    }
}

public final class LiquidLensView: UIView {
    public final class TransitionInfo {
        public let disableAnimationWorkarounds: Bool
        
        public init(disableAnimationWorkarounds: Bool) {
            self.disableAnimationWorkarounds = disableAnimationWorkarounds
        }
    }
    
    public enum Kind {
        case externalContainer
        case builtinContainer
        case noContainer
    }
    
    struct Params: Equatable {
        var size: CGSize
        var cornerRadius: CGFloat?
        var selectionOrigin: CGPoint
        var selectionSize: CGSize
        var inset: CGFloat
        var liftedInset: CGFloat
        var isDark: Bool
        var isLifted: Bool
        var isCollapsed: Bool

        init(size: CGSize, cornerRadius: CGFloat?, selectionOrigin: CGPoint, selectionSize: CGSize, inset: CGFloat, liftedInset: CGFloat, isDark: Bool, isLifted: Bool, isCollapsed: Bool) {
            self.size = size
            self.cornerRadius = cornerRadius
            self.selectionOrigin = selectionOrigin
            self.selectionSize = selectionSize
            self.inset = inset
            self.liftedInset = liftedInset
            self.isLifted = isLifted
            self.isDark = isDark
            self.isCollapsed = isCollapsed
        }
    }

    struct LensParams: Equatable {
        var baseFrame: CGRect
        var inset: CGFloat
        var liftedInset: CGFloat
        var isLifted: Bool

        init(baseFrame: CGRect, inset: CGFloat, liftedInset: CGFloat, isLifted: Bool) {
            self.baseFrame = baseFrame
            self.inset = inset
            self.liftedInset = liftedInset
            self.isLifted = isLifted
        }
    }

    let containerView: UIView
    let backgroundContainer: GlassBackgroundContainerView?
    let genericBackgroundContainer: UIView?
    let backgroundView: GlassBackgroundView?
    var lensView: UIView?
    let liftedContainerView: UIView
    public let contentView: UIView
    let restingBackgroundView: RestingBackgroundView
    
    var legacySelectionView: GlassBackgroundView.ContentImageView?
    var legacyContentMaskView: UIView?
    var legacyContentMaskBlobView: UIImageView?
    var legacyLiftedContentBlobMaskView: UIImageView?

    public var selectedContentView: UIView {
        return self.liftedContainerView
    }

    var params: Params?
    var appliedLensParams: LensParams?
    var isApplyingLensParams: Bool = false
    var pendingLensParams: LensParams?

    var liftedDisplayLink: SharedDisplayLinkDriver.Link?

    public var selectionOrigin: CGPoint? {
        return self.params?.selectionOrigin
    }

    public var selectionSize: CGSize? {
        return self.params?.selectionSize
    }
    
    public internal(set) var isAnimating: Bool = false {
        didSet {
            if self.isAnimating != oldValue {
                self.onUpdatedIsAnimating?(self.isAnimating)
            }
        }
    }
    public var onUpdatedIsAnimating: ((Bool) -> Void)?
    public var isLiftedAnimationCompleted: (() -> Void)?

    public init(kind: Kind) {
        self.containerView = UIView()
        
        switch kind {
        case .builtinContainer:
            self.backgroundContainer = GlassBackgroundContainerView()
            self.genericBackgroundContainer = nil
        case .externalContainer, .noContainer:
            self.backgroundContainer = nil
            self.genericBackgroundContainer = UIView()
        }
        
        if case .noContainer = kind {
            self.backgroundView = nil
        } else {
            self.backgroundView = GlassBackgroundView()
        }
        
        self.contentView = UIView()
        self.liftedContainerView = UIView()

        self.restingBackgroundView = RestingBackgroundView()

        super.init(frame: CGRect())
        
        if let backgroundContainer = self.backgroundContainer {
            self.addSubview(backgroundContainer)
            if let backgroundView = self.backgroundView {
                backgroundContainer.contentView.addSubview(backgroundView)
                backgroundView.contentView.addSubview(self.containerView)
            }
        } else if let genericBackgroundContainer = self.genericBackgroundContainer {
            self.addSubview(genericBackgroundContainer)
            if let backgroundView = self.backgroundView {
                genericBackgroundContainer.addSubview(backgroundView)
                backgroundView.contentView.addSubview(self.containerView)
            } else {
                genericBackgroundContainer.addSubview(self.containerView)
            }
        }
        self.containerView.isUserInteractionEnabled = false
        
        if #available(iOS 26.0, *) {
            if let viewClass = NSClassFromString("_UILiquidLensView") as AnyObject as? NSObjectProtocol {
                let allocSelector = NSSelectorFromString("alloc")
                let initSelector = NSSelectorFromString("initWithRestingBackground:")
                let objcAlloc = viewClass.perform(allocSelector).takeUnretainedValue()
                let instance = objcAlloc.perform(initSelector, with: UIView()).takeUnretainedValue()
                self.lensView = instance as? UIView
            }
        }
        
        if let lensView = self.lensView {
            if let backgroundContainer = self.backgroundContainer {
                backgroundContainer.layer.zPosition = 1
            } else if let genericBackgroundContainer = self.genericBackgroundContainer{
                genericBackgroundContainer.layer.zPosition = 1
            }
            lensView.layer.zPosition = 10.0
            
            self.liftedContainerView.addSubview(self.restingBackgroundView)
            
            self.containerView.addSubview(self.liftedContainerView)
            self.containerView.addSubview(lensView)
            self.containerView.addSubview(self.contentView)
            
            if let backgroundContainer = self.backgroundContainer {
                lensView.perform(NSSelectorFromString("setLiftedContainerView:"), with: backgroundContainer.contentView)
            } else if let genericBackgroundContainer = self.genericBackgroundContainer {
                lensView.perform(NSSelectorFromString("setLiftedContainerView:"), with: genericBackgroundContainer)
            }
            lensView.perform(NSSelectorFromString("setLiftedContentView:"), with: self.liftedContainerView)
            lensView.perform(NSSelectorFromString("setOverridePunchoutView:"), with: self.contentView)
            
            do {
                let selector = NSSelectorFromString("setLiftedContentMode:")
                if let method = lensView.method(for: selector) {
                    typealias ObjCMethod = @convention(c) (AnyObject, Selector, Int32) -> Void
                    let function = unsafeBitCast(method, to: ObjCMethod.self)
                    function(lensView, selector, 1)
                }
            }
            
            do {
                let selector = NSSelectorFromString("setStyle:")
                if let method = lensView.method(for: selector) {
                    typealias ObjCMethod = @convention(c) (AnyObject, Selector, Int32) -> Void
                    let function = unsafeBitCast(method, to: ObjCMethod.self)
                    function(lensView, selector, 1)
                }
            }
            
            do {
                let selector = NSSelectorFromString("setWarpsContentBelow:")
                if let method = lensView.method(for: selector) {
                    typealias ObjCMethod = @convention(c) (AnyObject, Selector, Bool) -> Void
                    let function = unsafeBitCast(method, to: ObjCMethod.self)
                    function(lensView, selector, true)
                }
            }
            
            lensView.setValue(UIColor(white: 0.0, alpha: 0.1), forKey: "restingBackgroundColor")
        } else {
            let legacySelectionView = GlassBackgroundView.ContentImageView()
            self.legacySelectionView = legacySelectionView
            if let backgroundView = self.backgroundView {
                backgroundView.contentView.insertSubview(legacySelectionView, at: 0)
            } else {
                self.containerView.insertSubview(legacySelectionView, at: 0)
            }
            
            let legacyContentMaskView = UIView()
            legacyContentMaskView.backgroundColor = .white
            self.legacyContentMaskView = legacyContentMaskView
            self.contentView.mask = legacyContentMaskView
            
            if let filter = CALayer.luminanceToAlpha() {
                legacyContentMaskView.layer.filters = [filter]
            }
            
            let legacyContentMaskBlobView = UIImageView()
            self.legacyContentMaskBlobView = legacyContentMaskBlobView
            legacyContentMaskView.addSubview(legacyContentMaskBlobView)
            
            self.containerView.addSubview(self.contentView)
            
            let legacyLiftedContentBlobMaskView = UIImageView()
            self.legacyLiftedContentBlobMaskView = legacyLiftedContentBlobMaskView
            self.liftedContainerView.mask = legacyLiftedContentBlobMaskView
            
            self.containerView.addSubview(self.liftedContainerView)
        }
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
    
    public func setLiftedContainer(view: UIView) {
        guard let lensView = self.lensView else {
            return
        }
        lensView.perform(NSSelectorFromString("setLiftedContainerView:"), with: view)
    }

    public func update(size: CGSize, cornerRadius: CGFloat? = nil, selectionOrigin: CGPoint, selectionSize: CGSize, inset: CGFloat, liftedInset: CGFloat = 4.0, isDark: Bool, isLifted: Bool, isCollapsed: Bool = false, transition: ComponentTransition) {
        let params = Params(size: size, cornerRadius: cornerRadius, selectionOrigin: selectionOrigin, selectionSize: selectionSize, inset: inset, liftedInset: liftedInset, isDark: isDark, isLifted: isLifted, isCollapsed: isCollapsed)
        if self.params == params {
            return
        }
        self.update(params: params, transition: transition)
    }

    func update(transition: ComponentTransition) {
        guard let params = self.params else {
            return
        }
        self.update(params: params, transition: transition)
    }

}
